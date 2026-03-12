struct PushConstants {
    params: vec4<f32>,
}

var<push_constant> push_constants: PushConstants;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );

    var output: VertexOutput;
    let position = positions[vertex_index];
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = position * 0.5 + vec2<f32>(0.5, 0.5);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let resolution = max(push_constants.params.xy, vec2<f32>(1.0, 1.0));
    let time = push_constants.params.z;
    let uv = input.uv;
    let centered = uv * 2.0 - vec2<f32>(1.0, 1.0);
    let radial = dot(centered, centered);

    let vignette = clamp(1.0 - pow(radial, 1.15), 0.0, 1.0);
    let edge_shadow = smoothstep(0.34, 1.02, radial);
    let scanlines = 0.88 + 0.12 * sin(uv.y * resolution.y * 3.14159265);
    let grille = 0.96 + 0.04 * sin(uv.x * resolution.x * 2.0943951);
    let flicker = 0.985 + 0.015 * sin(time * 40.0);
    let top_glass = smoothstep(0.48, 0.0, abs(centered.y + 0.72));

    let phosphor = vec3<f32>(0.08, 0.33, 0.14) * scanlines * grille * flicker;
    let glow = vec3<f32>(0.12, 0.22, 0.16) * top_glass * 0.18;
    let alpha = clamp(0.10 * scanlines + edge_shadow * 0.38, 0.0, 0.55);
    let color = phosphor * (0.25 + vignette * 0.2) + glow;

    return vec4<f32>(color, alpha);
}
