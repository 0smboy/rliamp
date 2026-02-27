#!/bin/zsh
GHOSTTY_DIR="$HOME/.config/ghostty"
GHOSTTY_CONF="$GHOSTTY_DIR/config"
TMUX_CONF="$HOME/.tmux.conf"
ZSHRC="$HOME/.zshrc"
STATIC_SHADER="$GHOSTTY_DIR/cyber-static.glsl"
CRAZY_SHADER="$GHOSTTY_DIR/cyber-crazy.glsl"
TOGGLE_SCRIPT="$GHOSTTY_DIR/veo-toggle.sh"
MARKER_START="# === RLIAMP VEO ENGINE START ==="
MARKER_END="# === RLIAMP VEO ENGINE END ==="

install_engine() {
    mkdir -p "$GHOSTTY_DIR"
    touch "$GHOSTTY_CONF" "$TMUX_CONF" "$ZSHRC"

    cat << 'INNER_EOF' > "$STATIC_SHADER"
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
INNER_EOF

    cat << 'INNER_EOF' > "$CRAZY_SHADER"
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec2 centered = uv - 0.5;
    float zoom = 1.0 - 0.3 * sin(iTime * 1.5); 
    float angle = sin(iTime * 0.5) * 0.1; 
    mat2 rot = mat2(cos(angle), -sin(angle), sin(angle), cos(angle));
    centered = rot * centered * zoom;
    vec2 final_uv = centered + 0.5;
    if(final_uv.x < 0.0 || final_uv.x > 1.0 || final_uv.y < 0.0 || final_uv.y > 1.0) { fragColor = vec4(0.0); return; }
    float amount = 0.01 * sin(iTime * 3.0); vec3 color;
    color.r = texture(iChannel0, vec2(final_uv.x + amount, final_uv.y)).r;
    color.g = texture(iChannel0, final_uv).g;
    color.b = texture(iChannel0, vec2(final_uv.x - amount, final_uv.y)).b;
    color *= 1.5;
    fragColor = vec4(color, 1.0);
}
INNER_EOF

    cat << 'INNER_EOF' > "$TOGGLE_SCRIPT"
#!/bin/zsh
CONF="$HOME/.config/ghostty/config"
if grep -q "cyber-static.glsl" "\$CONF"; then
    sed -i '' "s|cyber-static.glsl|cyber-crazy.glsl|g" "\$CONF"
    echo "🚀 暴走模式：3D 穿梭引擎已启动！"
elif grep -q "cyber-crazy.glsl" "\$CONF"; then
    sed -i '' 's|^custom-shader|# custom-shader|g' "\$CONF"
    echo "🛑 视觉引擎关闭：返回原生终端。"
elif grep -q "^# custom-shader" "\$CONF"; then
    sed -i '' 's|^# custom-shader|custom-shader|g' "\$CONF"
    sed -i '' "s|cyber-crazy.glsl|cyber-static.glsl|g" "\$CONF"
    echo "🟢 准备模式：CRT 辉光已就绪，请选择音乐。"
else
    echo "custom-shader = $HOME/.config/ghostty/cyber-static.glsl" >> "\$CONF"
    echo "🟢 初始化完成：进入静止态。"
fi
INNER_EOF
    chmod +x "$TOGGLE_SCRIPT"

    sed -i '' "/$MARKER_START/,/$MARKER_END/d" "$TMUX_CONF" 2>/dev/null
    sed -i '' "/$MARKER_START/,/$MARKER_END/d" "$ZSHRC" 2>/dev/null

    echo "$MARKER_START\nbind P display-popup -w 70% -h 60% -E \"rliamp\"\n$MARKER_END" >> "$TMUX_CONF"
    echo "$MARKER_START\nalias veo=\"$TOGGLE_SCRIPT\"\n$MARKER_END" >> "$ZSHRC"
    echo "安装完成！"
}

uninstall_engine() {
    rm -f "$STATIC_SHADER" "$CRAZY_SHADER" "$TOGGLE_SCRIPT"
    if [[ -f "$GHOSTTY_CONF" ]]; then sed -i '' '/^custom-shader/d; /^# custom-shader/d' "$GHOSTTY_CONF"; fi
    if [[ -f "$TMUX_CONF" ]]; then sed -i '' "/^$MARKER_START/,/^$MARKER_END/d" "$TMUX_CONF"; fi
    if [[ -f "$ZSHRC" ]]; then sed -i '' "/^$MARKER_START/,/^$MARKER_END/d" "$ZSHRC"; fi
    echo "卸载完成！系统已恢复如初。"
}

case "$1" in
    install) install_engine ;;
    uninstall) uninstall_engine ;;
esac
