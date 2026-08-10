use karaoke_services::AppServices;
use minijinja::Environment;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub services: AppServices,
    pub templates: Arc<Environment<'static>>,
}

pub fn load_templates(dir: &std::path::Path) -> anyhow::Result<Environment<'static>> {
    let mut env = Environment::new();
    for name in ["index.html", "playing.html", "song_edit.html"] {
        let path = dir.join(name);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("读取模板 {} 失败: {e}", path.display()))?;
        // 模板只在启动时加载一次并伴随进程生命周期，泄漏为 'static 换取 Environment<'static>。
        let leaked: &'static str = Box::leak(content.into_boxed_str());
        env.add_template(name, leaked)?;
    }
    Ok(env)
}
