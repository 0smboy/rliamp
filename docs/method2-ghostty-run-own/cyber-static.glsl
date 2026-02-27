void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec2 crtUV = uv * 2.0 - 1.0;
    vec2 offset = crtUV.yx / 5.0;
    crtUV = crtUV + crtUV * offset * offset;
    crtUV = crtUV * 0.5 + 0.5;
    if (crtUV.x < 0.0 || crtUV.x > 1.0 || crtUV.y < 0.0 || crtUV.y > 1.0) { fragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }
    float amount = 0.002; vec3 color;
    color.r = texture(iChannel0, vec2(crtUV.x + amount, crtUV.y)).r;
    color.g = texture(iChannel0, crtUV).g;
    color.b = texture(iChannel0, vec2(crtUV.x - amount, crtUV.y)).b;
    color -= sin(crtUV.y * 800.0 * 3.1415) * 0.04;
    color += sin(crtUV.y * 10.0 + iTime * 3.0) * 0.02;
    color += color * 0.3; 
    float vignette = crtUV.x * crtUV.y * (1.0 - crtUV.x) * (1.0 - crtUV.y);
    color *= clamp(pow(16.0 * vignette, 0.25), 0.0, 1.0);
    fragColor = vec4(color, 1.0);
}
