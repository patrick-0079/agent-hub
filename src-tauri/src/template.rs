//! `file.render` 的实现：Tera 模板渲染（Jinja2 兼容语法）。
//!
//! 这是 M3.5「定义模版语法」的内核原语：模板 + 上下文 → 文本。
//! 上下文由调用方构造（未来的 Adapter 会把 profile / provider / mcp /
//! env / secrets 塞进来）；渲染本身是纯函数，不碰磁盘。
//!
//! 未定义变量按 Tera 默认策略**报错**（宁可失败，也不静默输出空值）——
//! 密钥经由上下文进入渲染结果时，调用方负责展示层掩码。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRenderResult {
    pub ok: bool,
    pub output: String,
    pub error: String,
}

/// 渲染一段模板。`context` 是任意 JSON（对象 / 数组皆可）。
pub fn render(template: &str, context: &serde_json::Value) -> Result<String, String> {
    let mut tera = tera::Tera::default();
    tera.add_raw_template("inline", template)
        .map_err(|e| format!("模板语法错误：{}", e))?;
    let ctx = tera::Context::from_serialize(context)
        .map_err(|e| format!("上下文无法序列化：{}", e))?;
    tera.render("inline", &ctx).map_err(|e| format!("渲染失败：{}", e))
}

/// 便捷封装（GUI 试渲染工具用）：上下文传 JSON 文本，空串视为 `{}`。
pub fn render_checked(template: &str, context_json: &str) -> TemplateRenderResult {
    let context: serde_json::Value = if context_json.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(context_json) {
            Ok(v) => v,
            Err(e) => {
                return TemplateRenderResult {
                    ok: false,
                    error: format!("上下文不是合法 JSON：{}", e),
                    ..Default::default()
                }
            }
        }
    };
    match render(template, &context) {
        Ok(output) => TemplateRenderResult {
            ok: true,
            output,
            ..Default::default()
        },
        Err(error) => TemplateRenderResult {
            ok: false,
            error,
            ..Default::default()
        },
    }
}
