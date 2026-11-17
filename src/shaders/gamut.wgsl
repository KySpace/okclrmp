struct Camera { mvp: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
struct Output { @builtin(position) position:vec4<f32>, @location(0) color:vec3<f32>, @location(1) coord:vec3<f32> }
@vertex fn vs(@location(0) position:vec3<f32>, @location(1) color:vec3<f32>) -> Output {
    var out:Output;
    out.position=camera.mvp*vec4(position,1.0);
    out.color=color;
    out.coord=position;
    return out;
}
@fragment fn fs(in:Output) -> @location(0) vec4<f32> {
    let uv=vec2((in.coord.x+0.9)/1.8*12.0,(in.coord.z+0.65)/1.3*10.0);
    // Differentiate the continuous coordinate, not fract(), to keep a stable
    // screen-space line width at cell seams and during camera movement.
    let footprint=max(fwidth(uv),vec2(0.00001));
    let distance=abs(fract(uv+0.5)-0.5);
    let coverage=clamp(vec2(1.0)-distance/footprint,vec2(0.0),vec2(1.0));
    // Suppress an unresolved grid at grazing angles rather than producing moiré.
    let fade=vec2(1.0)-smoothstep(vec2(0.25),vec2(0.75),footprint);
    let grid=max(coverage.x*fade.x,coverage.y*fade.y);
    let grid_color=pow((vec3(0.82,0.89,0.94)+0.055)/1.055,vec3(2.4));
    return vec4(mix(in.color,grid_color,grid*0.26),1.0);
}
