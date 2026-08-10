//! HTTP Range 流式读取的底层实现（不依赖 axum，供 `karaoke-api` 组装响应）。
//! 对应 Python `karaoke/infra/streaming.py`。

use bytes::Bytes;
use futures::Stream;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const CHUNK_SIZE: usize = 65536;

#[derive(Debug, Clone, Copy)]
pub struct RangeSpec {
    pub start: u64,
    pub end: u64, // 闭区间
    pub is_partial: bool,
    pub file_size: u64,
}

impl RangeSpec {
    pub fn content_length(&self) -> u64 {
        self.end - self.start + 1
    }
}

/// 解析 `Range: bytes=start-end` 头，越界时回退到全量。
pub fn compute_range(file_size: u64, range_header: Option<&str>) -> RangeSpec {
    let Some(header) = range_header.filter(|h| h.starts_with("bytes=")) else {
        return RangeSpec {
            start: 0,
            end: file_size.saturating_sub(1),
            is_partial: false,
            file_size,
        };
    };
    let spec = &header["bytes=".len()..];
    let mut parts = spec.splitn(2, '-');
    let start: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let end: u64 = parts
        .next()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(file_size.saturating_sub(1))
        .min(file_size.saturating_sub(1));
    RangeSpec {
        start,
        end,
        is_partial: true,
        file_size,
    }
}

/// 以 64KiB 分片异步读取文件区间，供上层构建流式响应体。
pub fn stream_file_range(
    path: std::path::PathBuf,
    start: u64,
    end: u64,
) -> impl Stream<Item = std::io::Result<Bytes>> + Send + 'static {
    async_stream::try_stream! {
        let mut file = tokio::fs::File::open(&path).await?;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut remaining = end - start + 1;
        let mut buf = vec![0u8; CHUNK_SIZE];
        while remaining > 0 {
            let want = remaining.min(CHUNK_SIZE as u64) as usize;
            let n = file.read(&mut buf[..want]).await?;
            if n == 0 {
                break;
            }
            remaining -= n as u64;
            yield Bytes::copy_from_slice(&buf[..n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_range_header_returns_full_file() {
        let r = compute_range(1000, None);
        assert_eq!((r.start, r.end, r.is_partial), (0, 999, false));
    }

    #[test]
    fn open_ended_range_reads_to_eof() {
        let r = compute_range(1000, Some("bytes=500-"));
        assert_eq!((r.start, r.end, r.is_partial), (500, 999, true));
    }

    #[test]
    fn bounded_range_clamped_to_file_size() {
        let r = compute_range(1000, Some("bytes=100-2000"));
        assert_eq!((r.start, r.end), (100, 999));
        assert_eq!(r.content_length(), 900);
    }
}
