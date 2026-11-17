struct Camera {
    mvp: mat4x4<f32>, view:vec4<f32>, grid:vec4<f32>,
    slices:vec4<f32>, lightness_color:vec4<f32>, chroma_color:vec4<f32>,
    hue_color:vec4<f32>, contour_outline:vec4<f32>
}
@group(0) @binding(0) var<uniform> camera: Camera;
struct Output {
    @builtin(position) position:vec4<f32>, @location(0) color:vec3<f32>,
    @location(1) coord:vec3<f32>, @location(2) hue_cap:f32
}
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
    // All cap vertices lie on a chart edge. Interpolation distinguishes the
    // cap interior from the adjacent roof, whose edge vertices are shared.
    out.hue_cap=select(0.0,1.0,abs(abs(position.x)-0.9)<0.000001);
    return out;
}
// Distances divided by their pixel footprint keep contour width stable while
// orbiting and resizing. A coplanar chart cap is an area, not an intersection
// line: suppress its zero gradient rather than painting the whole face.
fn contour_coverage(distance:f32, footprint:f32, half_width:f32)->f32 {
    return select(0.0,clamp(half_width+0.5-abs(distance)/max(footprint,0.000001),0.0,1.0),footprint>0.000001);
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
    var color=mix(in.color,camera.grid.rgb,grid*0.26);
    if camera.slices.w>0.5 {
        var lightness=(in.coord.z+0.65)/1.3;
        var chroma=(in.coord.y+0.3)/1.2*0.4;
        let hue_angle=(((in.coord.x+0.9)/1.8+camera.view.x)-camera.slices.z)*6.28318530718;
        var hue_distance=sin(hue_angle);
        var hue_side=cos(hue_angle)>0.0;
        if camera.view.y>0.5 {
            lightness=(in.coord.y+0.9)/1.8;
            chroma=length(in.coord.xz)/1.2*0.4;
            let angle=(camera.slices.z-camera.view.x)*6.28318530718;
            let radial=vec2(cos(angle),sin(angle));
            hue_distance=dot(in.coord.xz,vec2(-radial.y,radial.x));
            // A hue slice is one radial half-plane, not also H+180 degrees.
            hue_side=dot(in.coord.xz,radial)>=0.0;
        } else if in.hue_cap>0.99999 {
            // Hue is constant over the chart cap. Explicitly exclude this
            // coplanar area, including floating-point derivative noise.
            hue_side=false;
        }
        let distance=vec3(lightness-camera.slices.x,chroma-camera.slices.y,hue_distance);
        let footprint=fwidth(distance);
        let outer=vec3(
            contour_coverage(distance.x,footprint.x,1.8),
            contour_coverage(distance.y,footprint.y,1.8),
            contour_coverage(distance.z,footprint.z,1.8)*select(0.0,1.0,hue_side));
        let inner=vec3(
            contour_coverage(distance.x,footprint.x,0.9),
            contour_coverage(distance.y,footprint.y,0.9),
            contour_coverage(distance.z,footprint.z,0.9)*select(0.0,1.0,hue_side));
        color=mix(color,camera.contour_outline.rgb,max(outer.x,max(outer.y,outer.z)));
        color=mix(color,camera.lightness_color.rgb,inner.x);
        color=mix(color,camera.chroma_color.rgb,inner.y);
        color=mix(color,camera.hue_color.rgb,inner.z);
    }
    return vec4(color,1.0);
}
