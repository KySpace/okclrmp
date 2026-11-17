struct Camera { mvp: mat4x4<f32>, view:vec4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
struct Output { @builtin(position) position:vec4<f32>, @location(0) color:vec3<f32>, @location(1) coord:vec3<f32> }
@vertex fn vs(@location(0) position:vec3<f32>, @location(1) color:vec3<f32>) -> Output {
    var out:Output;
    var world=position;
    if camera.view.y>0.5 {
        let hue=((position.x+0.9)/1.8-camera.view.x)*6.28318530718;
        let radius=position.y+0.3;
        world=vec3(radius*cos(hue),((position.z+0.65)/1.3)*1.8-0.9,radius*sin(hue));
    }
    out.position=camera.mvp*vec4(world,1.0);
    out.color=color;
    out.coord=world;
    return out;
}
@fragment fn fs(in:Output) -> @location(0) vec4<f32> {
    var uv=vec2(((in.coord.x+0.9)/1.8+camera.view.x)*12.0,(in.coord.z+0.65)/1.3*10.0);
    if camera.view.y>0.5 {uv=vec2((atan2(in.coord.z,in.coord.x)/6.28318530718+camera.view.x)*12.0,(in.coord.y+0.9)/1.8*10.0);}
    // Differentiate the continuous coordinate, not fract(), to keep a stable
    // screen-space line width at cell seams and during camera movement.
    var footprint=max(fwidth(uv),vec2(0.00001));
    // Angular grid coverage stays continuous at atan2's branch cut.
    if camera.view.y>0.5 {
        let r2=max(dot(in.coord.xz,in.coord.xz),0.000001);
        let dx=dpdx(in.coord.xz);let dy=dpdy(in.coord.xz);
        footprint.x=max((abs(in.coord.x*dx.y-in.coord.z*dx.x)+abs(in.coord.x*dy.y-in.coord.z*dy.x))*12.0/(r2*6.28318530718),0.00001);
    }
    let distance=asin(abs(sin(uv*3.14159265359)))/3.14159265359;
    let coverage=clamp(vec2(1.0)-distance/footprint,vec2(0.0),vec2(1.0));
    // Suppress an unresolved grid at grazing angles rather than producing moiré.
    let fade=vec2(1.0)-smoothstep(vec2(0.25),vec2(0.75),footprint);
    let grid=max(coverage.x*fade.x,coverage.y*fade.y);
    let grid_color=pow((vec3(0.82,0.89,0.94)+0.055)/1.055,vec3(2.4));
    return vec4(mix(in.color,grid_color,grid*0.26),1.0);
}
