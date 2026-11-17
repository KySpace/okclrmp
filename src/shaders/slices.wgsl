// Linear sRGB gamut test; display encoding occurs exactly once, below.
struct Params { size_plane: vec4<f32>, slice: vec4<f32> }
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var output: texture_storage_2d<rgba8unorm, write>;
fn linear_rgb(lch: vec3<f32>) -> vec3<f32> {
    let a = lch.y * cos(radians(lch.z));
    let b = lch.y * sin(radians(lch.z));
    let v = vec3(lch.x + 0.3963377774*a + 0.2158037573*b,
        lch.x - 0.1055613458*a - 0.0638541728*b,
        lch.x - 0.0894841775*a - 1.291485548*b);
    let q = v*v*v;
    return vec3(dot(q,vec3(4.0767416621,-3.3077115913,0.2309699292)),
        dot(q,vec3(-1.2684380046,2.6097574011,-0.3413193965)),
        dot(q,vec3(-0.0041960863,-0.7034186147,1.707614701)));
}
fn encode(v: vec3<f32>) -> vec3<f32> {
    let a = max(v,vec3(0.0));
    return select(1.055*pow(a,vec3(1.0/2.4))-0.055,12.92*a,a<=vec3(0.0031308));
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=u32(p.size_plane.x) || id.y>=u32(p.size_plane.y) { return; }
    let uv = (vec2<f32>(id.xy)+0.5)/p.size_plane.xy;
    var lch = vec3(uv.x,(1.0-uv.y)*0.4,p.slice.z);
    if p.size_plane.z>0.5 && p.size_plane.z<1.5 { lch = vec3(p.slice.x,(1.0-uv.y)*0.4,uv.x*360.0); }
    if p.size_plane.z>1.5 { lch = vec3(1.0-uv.y,p.slice.y,uv.x*360.0); }
    let rgb = linear_rgb(lch);
    let valid = all(rgb>=vec3(-0.00001)) && all(rgb<=vec3(1.00001));
    let tile = f32((id.x/8u + id.y/8u)%2u);
    var color = mix(vec3(0.085,0.098,0.12),vec3(0.102,0.116,0.14),tile);
    if valid { color = encode(clamp(rgb,vec3(0.0),vec3(1.0))); }
    // Fine graticule, deliberately subdued over the color field.
    let gx = abs(fract(uv.x*4.0+0.5)-0.5)*p.size_plane.x/4.0;
    let gy = abs(fract(uv.y*4.0+0.5)-0.5)*p.size_plane.y/4.0;
    if min(gx,gy)<0.45 { color=mix(color,vec3(0.68,0.74,0.8),0.15); }
    textureStore(output,vec2<i32>(id.xy),vec4(color,1.0));
}
