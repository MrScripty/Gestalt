use super::EmilyRuntime;
use crate::error::EmilyError;
use crate::model::{
    ContextItem, ContextPacket, ContextQuery, TextEdge, TextEdgeType, TextObject, TextVector,
};
use crate::store::EmilyStore;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

const CONFIDENCE_BASE_WEIGHT: f32 = 0.4;
const CONFIDENCE_DYNAMIC_WEIGHT: f32 = 0.6;
const LEARNING_BASE_WEIGHT: f32 = 0.5;
const LEARNING_DYNAMIC_WEIGHT: f32 = 0.5;
const SEMANTIC_QUERY_WEIGHT: f32 = 0.85;
const LEXICAL_QUERY_WEIGHT: f32 = 0.15;
const NEIGHBOR_DEPTH_DECAY: f32 = 0.8;

#[derive(Debug, Clone)]
struct CandidateScore {
    similarity: f32,
    rank: f32,
    provenance: Vec<String>,
}

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    pub(super) async fn maybe_link_semantic_edges(
        &self,
        object: &TextObject,
        vector: &TextVector,
    ) -> Result<(), EmilyError> {
        let policy = self.policy.read().await.clone();
        let mut candidates = self
            .store
            .list_text_vectors(Some(&object.stream_id))
            .await?
            .into_iter()
            .filter(|candidate| candidate.object_id != object.id)
            .filter(|candidate| candidate.dimensions == vector.dimensions)
            .filter_map(|candidate| {
                cosine_similarity(&vector.vector, &candidate.vector)
                    .map(|similarity| (candidate.object_id, similarity, candidate.ts_unix_ms))
            })
            .filter(|(_, similarity, _)| *similarity >= policy.semantic_min_similarity)
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| right.2.cmp(&left.2))
        });
        candidates.truncate(policy.semantic_top_k);

        for (candidate_id, similarity, _) in candidates {
            self.store
                .upsert_text_edge(&TextEdge {
                    id: format!("edge:semantic:{}:{}", object.id, candidate_id),
                    from_id: object.id.clone(),
                    to_id: candidate_id.clone(),
                    edge_type: TextEdgeType::SemanticSimilar,
                    weight: similarity,
                    ts_unix_ms: object.ts_unix_ms,
                })
                .await?;
            self.store
                .upsert_text_edge(&TextEdge {
                    id: format!("edge:semantic:{}:{}", candidate_id, object.id),
                    from_id: candidate_id,
                    to_id: object.id.clone(),
                    edge_type: TextEdgeType::SemanticSimilar,
                    weight: similarity,
                    ts_unix_ms: object.ts_unix_ms,
                })
                .await?;
        }

        Ok(())
    }

    pub(super) async fn query_context_internal(
        &self,
        query: ContextQuery,
    ) -> Result<ContextPacket, EmilyError> {
        if query.query_text.trim().is_empty() {
            return Err(EmilyError::InvalidRequest(
                "query_text cannot be empty".to_string(),
            ));
        }
        if query.top_k == 0 {
            return Err(EmilyError::InvalidRequest(
                "top_k must be greater than zero".to_string(),
            ));
        }

        let policy = self.policy.read().await.clone();
        let objects = self
            .store
            .list_text_objects(query.stream_id.as_deref())
            .await?;
        if objects.is_empty() {
            return Ok(ContextPacket { items: Vec::new() });
        }

        let vectors = self
            .store
            .list_text_vectors(query.stream_id.as_deref())
            .await?;
        let vector_by_object_id = vectors
            .into_iter()
            .map(|vector| (vector.object_id.clone(), vector))
            .collect::<HashMap<_, _>>();
        let object_by_id = objects
            .iter()
            .cloned()
            .map(|object| (object.id.clone(), object))
            .collect::<HashMap<_, _>>();
        let latest_ts_unix_ms = objects
            .iter()
            .map(|object| object.ts_unix_ms)
            .max()
            .unwrap_or(0);
        let query_vector = self.embed_query_vector(&query).await;

        let mut ranked = objects
            .iter()
            .map(|object| {
                let lexical = lexical_similarity(&query.query_text, &object.text);
                let semantic = query_vector
                    .as_ref()
                    .and_then(|query_vector| {
                        vector_by_object_id
                            .get(&object.id)
                            .and_then(|vector| cosine_similarity(query_vector, &vector.vector))
                    })
                    .unwrap_or(0.0);
                let similarity = combined_similarity(query_vector.is_some(), semantic, lexical);
                let rank = rank_score(
                    similarity,
                    object,
                    latest_ts_unix_ms,
                    policy.recency_decay_half_life_s,
                );
                (
                    object.id.clone(),
                    CandidateScore {
                        similarity,
                        rank,
                        provenance: vec![object.id.clone()],
                    },
                )
            })
            .collect::<Vec<_>>();

        ranked.sort_by(|left, right| {
            right
                .1
                .rank
                .partial_cmp(&left.1.rank)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    let right_sequence = object_by_id
                        .get(&right.0)
                        .map(|object| object.sequence)
                        .unwrap_or(0);
                    let left_sequence = object_by_id
                        .get(&left.0)
                        .map(|object| object.sequence)
                        .unwrap_or(0);
                    right_sequence.cmp(&left_sequence)
                })
        });

        let seed_count = usize::max(query.top_k, policy.semantic_top_k);
        let seed_ids = ranked
            .iter()
            .take(seed_count)
            .map(|(object_id, _)| object_id.clone())
            .collect::<Vec<_>>();

        let mut best_scores = ranked.into_iter().collect::<HashMap<_, _>>();
        if query.neighbor_depth > 0 && !seed_ids.is_empty() {
            let edges = self
                .store
                .list_text_edges(&seed_ids, query.neighbor_depth)
                .await?;
            let adjacency = build_edge_adjacency(&edges);

            for seed_id in seed_ids {
                let Some(seed_score) = best_scores.get(&seed_id).cloned() else {
                    continue;
                };
                let mut frontier = VecDeque::from([(seed_id.clone(), seed_score, 0_u8)]);
                while let Some((current_id, current_score, depth)) = frontier.pop_front() {
                    if depth >= query.neighbor_depth {
                        continue;
                    }
                    let Some(neighbors) = adjacency.get(&current_id) else {
                        continue;
                    };
                    for edge in neighbors {
                        let next_depth = depth.saturating_add(1);
                        let mut provenance = current_score.provenance.clone();
                        provenance.push(edge.to_id.clone());
                        let candidate = CandidateScore {
                            similarity: current_score.similarity * edge.weight,
                            rank: current_score.rank
                                * edge.weight
                                * NEIGHBOR_DEPTH_DECAY.powi(next_depth as i32),
                            provenance,
                        };
                        let replace = best_scores
                            .get(&edge.to_id)
                            .is_none_or(|existing| candidate.rank > existing.rank);
                        if replace {
                            best_scores.insert(edge.to_id.clone(), candidate.clone());
                        }
                        frontier.push_back((edge.to_id.clone(), candidate, next_depth));
                    }
                }
            }
        }

        let mut items = best_scores
            .into_iter()
            .filter_map(|(object_id, score)| {
                object_by_id
                    .get(&object_id)
                    .cloned()
                    .map(|object| ContextItem {
                        object,
                        similarity: score.similarity,
                        rank: score.rank,
                        provenance: score.provenance,
                    })
            })
            .collect::<Vec<_>>();

        items.sort_by(|left, right| {
            right
                .rank
                .partial_cmp(&left.rank)
                .unwrap_or(Ordering::Equal)
                .then_with(|| right.object.sequence.cmp(&left.object.sequence))
        });
        items.truncate(query.top_k);

        Ok(ContextPacket { items })
    }

    async fn embed_query_vector(&self, query: &ContextQuery) -> Option<Vec<f32>> {
        let config = self.vectorization.read().await.config.clone();
        if !config.enabled {
            return None;
        }
        let provider = self.embedding_provider.as_ref()?;
        let vector = provider.embed_text(&query.query_text).await.ok()?;
        if vector.is_empty() || vector.len() != config.expected_dimensions {
            return None;
        }
        if vector.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(vector)
    }
}

fn build_edge_adjacency(edges: &[TextEdge]) -> HashMap<String, Vec<TextEdge>> {
    let mut adjacency = HashMap::<String, Vec<TextEdge>>::new();
    for edge in edges {
        adjacency
            .entry(edge.from_id.clone())
            .or_default()
            .push(edge.clone());
        adjacency
            .entry(edge.to_id.clone())
            .or_default()
            .push(TextEdge {
                id: edge.id.clone(),
                from_id: edge.to_id.clone(),
                to_id: edge.from_id.clone(),
                edge_type: edge.edge_type,
                weight: edge.weight,
                ts_unix_ms: edge.ts_unix_ms,
            });
    }
    adjacency
}

fn combined_similarity(
    has_query_vector: bool,
    semantic_similarity: f32,
    lexical_similarity: f32,
) -> f32 {
    if has_query_vector {
        (semantic_similarity * SEMANTIC_QUERY_WEIGHT) + (lexical_similarity * LEXICAL_QUERY_WEIGHT)
    } else {
        lexical_similarity
    }
}

fn rank_score(
    similarity: f32,
    object: &TextObject,
    latest_ts_unix_ms: i64,
    recency_decay_half_life_s: u64,
) -> f32 {
    similarity
        * (CONFIDENCE_BASE_WEIGHT + (CONFIDENCE_DYNAMIC_WEIGHT * object.confidence))
        * (LEARNING_BASE_WEIGHT + (LEARNING_DYNAMIC_WEIGHT * object.learning_weight))
        * recency_decay(
            object.ts_unix_ms,
            latest_ts_unix_ms,
            recency_decay_half_life_s,
        )
}

fn recency_decay(object_ts_unix_ms: i64, latest_ts_unix_ms: i64, half_life_s: u64) -> f32 {
    if half_life_s == 0 {
        return 1.0;
    }
    let age_ms = latest_ts_unix_ms.saturating_sub(object_ts_unix_ms).max(0) as f64;
    let half_life_ms = (half_life_s as f64) * 1_000.0;
    (0.5_f64).powf(age_ms / half_life_ms) as f32
}

fn lexical_similarity(query: &str, text: &str) -> f32 {
    let query_tokens = parse_query_tokens(query);
    let text_tokens = parse_query_tokens(text);
    if query_tokens.is_empty() || text_tokens.is_empty() {
        return 0.0;
    }

    let overlap = query_tokens
        .iter()
        .map(|(token, q_count)| {
            let t_count = text_tokens.get(token).copied().unwrap_or(0);
            usize::min(*q_count, t_count)
        })
        .sum::<usize>();
    let query_norm = query_tokens.values().copied().sum::<usize>();
    let text_norm = text_tokens.values().copied().sum::<usize>();
    if query_norm == 0 || text_norm == 0 {
        return 0.0;
    }
    (2.0 * overlap as f32) / (query_norm + text_norm) as f32
}

fn parse_query_tokens(value: &str) -> HashMap<String, usize> {
    let mut freq = HashMap::<String, usize>::new();
    for token in value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        *freq.entry(token.to_ascii_lowercase()).or_insert(0) += 1;
    }
    freq
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len() || left.is_empty() {
        return None;
    }

    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm <= f32::EPSILON || right_norm <= f32::EPSILON {
        return None;
    }
    Some(dot / (left_norm.sqrt() * right_norm.sqrt()))
}
