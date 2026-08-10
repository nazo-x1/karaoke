//! 兼容前端历史写法（如 `?wait=1`）的宽松布尔查询参数解析。
//! Starlette/FastAPI 对 query bool 的转换本就宽松（接受 "1"/"true"/"yes"/"on" 等），
//! `serde_urlencoded` 默认只接受严格的 `"true"`/`"false"`，这里补齐等价行为。

use serde::{Deserialize, Deserializer};

pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ))
}

pub fn deserialize_optional<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.map(|s| {
        matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(deserialize_with = "deserialize")]
        wait: bool,
    }

    #[test]
    fn accepts_legacy_numeric_and_word_forms() {
        for (input, expected) in [
            ("1", true),
            ("true", true),
            ("TRUE", true),
            ("yes", true),
            ("on", true),
            ("0", false),
            ("false", false),
            ("", false),
            ("nope", false),
        ] {
            let v: Wrapper = serde_urlencoded::from_str(&format!("wait={input}")).unwrap();
            assert_eq!(v.wait, expected, "input={input}");
        }
    }
}
