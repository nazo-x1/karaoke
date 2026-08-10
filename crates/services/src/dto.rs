//! 统一 API 响应 envelope。对应 Python `karaoke/dto/api_result.py`。
//! 字段名/大小写（含 `totalPage`）与默认值必须与 V1 保持一致，前端零改动依赖这份契约。

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ApiResult {
    pub code: i32,
    pub msg: String,
    pub data: Value,
    pub total: i64,
    pub page: i64,
    #[serde(rename = "totalPage")]
    pub total_page: i64,
}

impl Default for ApiResult {
    fn default() -> Self {
        Self {
            code: 0,
            msg: "Success!".to_string(),
            data: Value::Null,
            total: 0,
            page: 0,
            total_page: 0,
        }
    }
}

impl ApiResult {
    pub fn ok() -> Self {
        Self::default()
    }

    pub fn ok_with_data<T: Serialize>(data: T) -> Self {
        Self {
            data: serde_json::to_value(data).unwrap_or(Value::Null),
            ..Self::default()
        }
    }

    pub fn ok_msg(msg: impl Into<String>) -> Self {
        Self {
            msg: msg.into(),
            ..Self::default()
        }
    }

    pub fn ok_msg_data<T: Serialize>(msg: impl Into<String>, data: T) -> Self {
        Self {
            msg: msg.into(),
            data: serde_json::to_value(data).unwrap_or(Value::Null),
            ..Self::default()
        }
    }

    pub fn fail(msg: impl Into<String>) -> Self {
        Self {
            code: 1,
            msg: msg.into(),
            ..Self::default()
        }
    }

    pub fn fail_with_data<T: Serialize>(msg: impl Into<String>, data: T) -> Self {
        Self {
            code: 1,
            msg: msg.into(),
            data: serde_json::to_value(data).unwrap_or(Value::Null),
            ..Self::default()
        }
    }

    pub fn not_found(label: &str) -> Self {
        Self::fail(format!("{label}不存在"))
    }

    pub fn with_pagination(mut self, total: i64, page: i64, page_size: i64) -> Self {
        self.total = total;
        self.page = page;
        self.total_page = if total > 0 {
            (total + page_size - 1) / page_size
        } else {
            0
        };
        self
    }
}

/// 对应 Python `errors.format_api_error` 中数据库相关的错误文本归一化；
/// 完整错误链交给 `tracing::error!` 记录，向前端只暴露归一化后的提示语。
pub fn db_error_message(err: &sqlx::Error, action: &str) -> String {
    tracing::error!("{action} failed: {err:?}");
    match err {
        sqlx::Error::RowNotFound => format!("{action}：资源不存在"),
        _ => {
            let text = err.to_string().to_lowercase();
            if text.contains("undefined column") {
                format!("{action}：数据库字段不完整，请重启服务以自动迁移")
            } else if text.contains("undefined table")
                || (text.contains("relation") && text.contains("does not exist"))
            {
                format!("{action}：数据库表缺失，请重启服务初始化")
            } else {
                format!("{action}：数据库访问异常")
            }
        }
    }
}

pub fn io_error_message(err: &std::io::Error, action: &str) -> String {
    tracing::error!("{action} failed: {err:?}");
    format!("{action}：{err}")
}
