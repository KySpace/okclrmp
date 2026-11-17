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
    let hline=abs(fract((in.coord.x+0.9)/1.8*12.0+0.5)-0.5);
    let lline=abs(fract((in.coord.z+0.65)/1.3*10.0+0.5)-0.5);
    let line=min(hline,lline);
    let grid=1.0-smoothstep(0.005,0.018,line);
    return vec4(mix(in.color,vec3(0.82,0.89,0.94),grid*0.26),1.0);
}
