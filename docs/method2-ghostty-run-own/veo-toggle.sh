#!/bin/zsh
CONF="$HOME/.config/ghostty/config"

# 1. 如果是被注释状态 (OFF)，则开启并重置为静止态
if grep -q "^# custom-shader" "$CONF"; then
    sed -i '' 's|^# custom-shader|custom-shader|g' "$CONF"
    sed -i '' "s|cyber-crazy.glsl|cyber-static.glsl|g" "$CONF"
    echo "🟢 准备模式：CRT 辉光已就绪，请选择音乐。"

# 2. 如果是开启状态，且当前是静止态，则切换到暴走态
elif grep -q "^custom-shader.*cyber-static.glsl" "$CONF"; then
    sed -i '' "s|cyber-static.glsl|cyber-crazy.glsl|g" "$CONF"
    echo "🚀 暴走模式：3D 穿梭引擎已启动！"

# 3. 如果是开启状态，且当前已经是暴走态，则关闭特效
elif grep -q "^custom-shader.*cyber-crazy.glsl" "$CONF"; then
    sed -i '' 's|^custom-shader|# custom-shader|g' "$CONF"
    echo "🛑 视觉引擎关闭：返回原生终端。"

# 4. 如果配置文件里根本没有这一行，则初始化
else
    echo "custom-shader = $HOME/.config/ghostty/cyber-static.glsl" >> "$CONF"
    echo "🟢 初始化完成：进入静止态。"
fi
