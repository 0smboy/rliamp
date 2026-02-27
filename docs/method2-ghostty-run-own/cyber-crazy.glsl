void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec2 centered = uv - 0.5;

    // 1. 制造强烈的心跳节拍 (0.0 到 1.0 之间跳动)
    float beat = (sin(iTime * 2.5) + 1.0) * 0.5; 
    
    // 2. 核心魔法：计算当前像素到屏幕正中心的距离
    float dist = length(centered);
    
    // 3. 3D 深度膨胀 (Spatial Bulge)
    // 设定：中心最高放大近 2 倍 (0.55)，边缘保持原比例 (1.0)
    float centerScale = 1.0 - 0.45 * beat; 
    float edgeScale = 1.0; 
    
    // 使用 mix 函数：越靠近中心，采用 centerScale；越靠近边缘，采用 edgeScale
    // 这会让中间的 rliamp 框像一颗心脏一样猛烈地钻出屏幕！
    float zDepth = mix(centerScale, edgeScale, dist * 1.5); 
    
    // 加入极其轻微的 Z 轴摇晃
    float angle = sin(iTime * 1.0) * 0.02; 
    mat2 rot = mat2(cos(angle), -sin(angle), sin(angle), cos(angle));

    // 应用形变
    centered = rot * centered * zDepth;
    vec2 final_uv = centered + 0.5;

    // 4. 固定在屏幕玻璃上的暗角 (防止画面撕裂露出黑边)
    float edgeFade = smoothstep(0.5, 0.3, abs(uv.x - 0.5)) * smoothstep(0.5, 0.3, abs(uv.y - 0.5));
    
    if(final_uv.x < 0.0 || final_uv.x > 1.0 || final_uv.y < 0.0 || final_uv.y > 1.0) { 
        fragColor = vec4(0.0); 
        return; 
    }

    // 5. 带有重影的 RGB 色散分离 (色散也会随着心跳节奏变大变小)
    float amount = 0.006 * beat; 
    vec3 color;
    color.r = texture(iChannel0, vec2(final_uv.x + amount, final_uv.y)).r;
    color.g = texture(iChannel0, final_uv).g;
    color.b = texture(iChannel0, vec2(final_uv.x - amount, final_uv.y)).b;
    
    // 应用暗角，并让亮度也跟着呼吸闪烁
    color *= edgeFade;
    color *= 1.1 + 0.4 * beat;

    fragColor = vec4(color, 1.0);
}
