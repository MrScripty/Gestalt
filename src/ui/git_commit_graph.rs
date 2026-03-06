use crate::git::CommitInfo;

pub(crate) const GRAPH_ROW_HEIGHT_PX: f32 = 56.0;
pub(crate) const GRAPH_LANE_WIDTH_PX: f32 = 16.0;
pub(crate) const GRAPH_GUTTER_PADDING_PX: f32 = 8.0;
pub(crate) const GRAPH_NODE_RADIUS_PX: f32 = 4.5;

#[derive(Clone, Debug)]
pub(crate) struct GraphSegment {
    pub from_lane: usize,
    pub to_lane: usize,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub is_merge: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct GraphNode {
    pub sha: String,
    pub lane: usize,
    pub x: f32,
    pub y: f32,
    pub is_unpushed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CommitGraphLayout {
    pub lane_count: usize,
    pub gutter_width_px: f32,
    pub overlay_height_px: f32,
    pub summary_x: f32,
    pub summary_y: f32,
    pub nodes: Vec<GraphNode>,
    pub segments: Vec<GraphSegment>,
}

pub(crate) fn build_commit_graph_layout(commits: &[CommitInfo]) -> CommitGraphLayout {
    let mut active_lanes = Vec::<Option<String>>::new();
    let mut nodes = Vec::<GraphNode>::new();
    let mut segments = Vec::<GraphSegment>::new();
    let mut max_lane = 0_usize;

    for (commit_idx, commit) in commits.iter().enumerate() {
        let commit_lane = find_or_insert_lane(&mut active_lanes, &commit.sha);
        clear_duplicate_lanes(&mut active_lanes, commit_lane, &commit.sha);
        let lanes_before = active_lanes.clone();

        let mut parent_lanes = Vec::<usize>::new();
        if commit.parents.is_empty() {
            active_lanes[commit_lane] = None;
        } else {
            let first_parent = commit.parents[0].clone();
            let first_parent_lane = find_existing_lane(&active_lanes, &first_parent);
            if let Some(parent_lane) = first_parent_lane {
                parent_lanes.push(parent_lane);
                if parent_lane != commit_lane {
                    active_lanes[commit_lane] = None;
                }
            } else {
                active_lanes[commit_lane] = Some(first_parent);
                parent_lanes.push(commit_lane);
            }

            for parent_sha in commit.parents.iter().skip(1) {
                let lane = find_or_insert_lane(&mut active_lanes, parent_sha);
                active_lanes[lane] = Some(parent_sha.clone());
                parent_lanes.push(lane);
            }
        }

        let lanes_after = active_lanes.clone();
        max_lane = max_lane.max(commit_lane);
        for lane in &parent_lanes {
            max_lane = max_lane.max(*lane);
        }

        nodes.push(GraphNode {
            sha: commit.sha.clone(),
            lane: commit_lane,
            x: lane_center_x(commit_lane),
            y: row_center_y(commit_idx + 1),
            is_unpushed: commit.is_unpushed,
        });

        let has_next_row = commit_idx + 1 < commits.len();
        if has_next_row {
            let lane_len = lanes_before.len().max(lanes_after.len());
            for lane in 0..lane_len {
                let has_before = lanes_before
                    .get(lane)
                    .and_then(|value| value.as_ref())
                    .is_some();
                let has_after = lanes_after
                    .get(lane)
                    .and_then(|value| value.as_ref())
                    .is_some();
                if has_before && has_after {
                    segments.push(GraphSegment {
                        from_lane: lane,
                        to_lane: lane,
                        x1: lane_center_x(lane),
                        y1: row_center_y(commit_idx + 1),
                        x2: lane_center_x(lane),
                        y2: row_center_y(commit_idx + 2),
                        is_merge: false,
                    });
                }
            }

            for parent_lane in parent_lanes {
                if parent_lane != commit_lane {
                    segments.push(GraphSegment {
                        from_lane: commit_lane,
                        to_lane: parent_lane,
                        x1: lane_center_x(commit_lane),
                        y1: row_center_y(commit_idx + 1),
                        x2: lane_center_x(parent_lane),
                        y2: row_center_y(commit_idx + 2),
                        is_merge: true,
                    });
                }
            }
        }
    }

    let lane_count = (max_lane + 1).max(1);
    CommitGraphLayout {
        lane_count,
        gutter_width_px: graph_gutter_width_px(lane_count),
        overlay_height_px: GRAPH_ROW_HEIGHT_PX * (commits.len() + 1) as f32,
        summary_x: lane_center_x(0),
        summary_y: row_center_y(0),
        nodes,
        segments,
    }
}

fn find_existing_lane(active_lanes: &[Option<String>], sha: &str) -> Option<usize> {
    active_lanes
        .iter()
        .position(|lane| lane.as_deref() == Some(sha))
}

fn find_or_insert_lane(active_lanes: &mut Vec<Option<String>>, sha: &str) -> usize {
    if let Some(existing) = find_existing_lane(active_lanes, sha) {
        return existing;
    }
    if let Some(free) = active_lanes.iter().position(|lane| lane.is_none()) {
        active_lanes[free] = Some(sha.to_string());
        return free;
    }
    active_lanes.push(Some(sha.to_string()));
    active_lanes.len() - 1
}

fn clear_duplicate_lanes(active_lanes: &mut [Option<String>], keep_lane: usize, sha: &str) {
    for (idx, lane) in active_lanes.iter_mut().enumerate() {
        if idx != keep_lane && lane.as_deref() == Some(sha) {
            *lane = None;
        }
    }
}

fn lane_center_x(lane: usize) -> f32 {
    GRAPH_GUTTER_PADDING_PX + GRAPH_LANE_WIDTH_PX * lane as f32 + (GRAPH_LANE_WIDTH_PX / 2.0)
}

fn row_center_y(row_index: usize) -> f32 {
    GRAPH_ROW_HEIGHT_PX * (row_index as f32 + 0.5)
}

pub(crate) fn graph_gutter_width_px(lane_count: usize) -> f32 {
    GRAPH_GUTTER_PADDING_PX * 2.0 + GRAPH_LANE_WIDTH_PX * lane_count.max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::build_commit_graph_layout;
    use crate::git::CommitInfo;

    #[test]
    fn linear_history_uses_one_lane() {
        let commits = vec![
            commit("a", &["b"], false),
            commit("b", &["c"], false),
            commit("c", &[], false),
        ];

        let layout = build_commit_graph_layout(&commits);
        assert_eq!(layout.lane_count, 1);
        assert_eq!(layout.nodes.len(), 3);
        assert!(layout.segments.iter().all(|segment| !segment.is_merge));
    }

    #[test]
    fn merge_history_creates_multiple_lanes_and_diagonals() {
        let commits = vec![
            commit("m", &["a", "b"], false),
            commit("a", &["c"], false),
            commit("b", &["c"], false),
            commit("c", &[], false),
        ];

        let layout = build_commit_graph_layout(&commits);
        assert!(layout.lane_count >= 2);
        assert!(layout.segments.iter().any(|segment| segment.is_merge));
        assert!(
            layout
                .segments
                .iter()
                .any(|segment| segment.from_lane != segment.to_lane)
        );
    }

    fn commit(sha: &str, parents: &[&str], is_unpushed: bool) -> CommitInfo {
        CommitInfo {
            sha: sha.to_string(),
            short_sha: sha.to_string(),
            author: "author".to_string(),
            authored_at: "2026-03-01T00:00:00+00:00".to_string(),
            subject: format!("subject {sha}"),
            body_preview: String::new(),
            decorations: Vec::new(),
            graph_prefix: String::new(),
            parents: parents.iter().map(|value| (*value).to_string()).collect(),
            is_unpushed,
        }
    }
}
