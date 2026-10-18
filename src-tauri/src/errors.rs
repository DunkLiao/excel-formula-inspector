use serde::Serialize;

/// 掃描／讀檔的結構化錯誤。
///
/// 原則（SPEC §8.2）：
/// - 對外只回傳可理解的訊息，不帶內部堆疊或完整本機路徑。
/// - 每個錯誤都有穩定的機器可讀 `code`，供前端決定 UI 行為。
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("找不到檔案，可能已被移動或刪除。")]
    FileNotFound,

    #[error("選擇的路徑不是檔案。")]
    NotAFile,

    #[error("不支援的副檔名「{0}」。MVP 僅支援 .xlsx 與 .xlsm。")]
    UnsupportedFormat(String),

    /// 超過預設上限但未確認 → 前端需詢問使用者是否以實驗性模式繼續（SPEC §8.2）。
    #[error("檔案大小 {size_mb:.1} MB 超過預設上限 20 MB。")]
    LargeFileNeedsConfirmation { size_bytes: u64, size_mb: f64 },

    #[error("檔案大小 {size_mb:.1} MB 超過硬上限 {limit_mb} MB，為避免記憶體耗盡已拒絕處理。")]
    TooLarge {
        size_bytes: u64,
        size_mb: f64,
        limit_mb: u64,
    },

    #[error("已使用的儲存格數量 {cells} 超過上限 {limit}，為避免記憶體耗盡已拒絕處理。")]
    TooManyCells { cells: usize, limit: usize },

    #[error("此檔案受密碼保護或已加密，無法解析。")]
    Encrypted,

    #[error("檔案損毀或格式不正確，無法解析。")]
    Corrupt { detail: String },

    #[error("讀取檔案失敗。")]
    Io { detail: String },

    #[error("找不到掃描結果，可能已被清除。")]
    ScanNotFound,

    #[error("掃描已取消。")]
    Cancelled,

    #[error("內部錯誤，請重新嘗試。")]
    Internal { detail: String },
}

impl ScanError {
    /// 穩定的錯誤碼，供前端 switch。
    pub fn code(&self) -> &'static str {
        match self {
            ScanError::FileNotFound => "FILE_NOT_FOUND",
            ScanError::NotAFile => "NOT_A_FILE",
            ScanError::UnsupportedFormat(_) => "UNSUPPORTED_FORMAT",
            ScanError::LargeFileNeedsConfirmation { .. } => "LARGE_FILE_NEEDS_CONFIRMATION",
            ScanError::TooLarge { .. } => "TOO_LARGE",
            ScanError::TooManyCells { .. } => "TOO_MANY_CELLS",
            ScanError::Encrypted => "ENCRYPTED",
            ScanError::Corrupt { .. } => "CORRUPT",
            ScanError::Io { .. } => "IO_ERROR",
            ScanError::ScanNotFound => "SCAN_NOT_FOUND",
            ScanError::Cancelled => "CANCELLED",
            ScanError::Internal { .. } => "INTERNAL",
        }
    }

    /// 前端顯示用資訊；`sizeBytes` 等結構化欄位讓 UI 不必解析字串。
    pub fn payload(&self) -> ErrorPayload {
        let extra = match self {
            ScanError::LargeFileNeedsConfirmation { size_bytes, .. } => {
                Some(ErrorExtra::Size { size_bytes: *size_bytes })
            }
            ScanError::TooLarge { size_bytes, .. } => {
                Some(ErrorExtra::Size { size_bytes: *size_bytes })
            }
            ScanError::TooManyCells { limit, .. } => Some(ErrorExtra::Limit { limit: *limit }),
            _ => None,
        };
        ErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
            extra,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<ErrorExtra>,
}

impl From<ScanError> for ErrorPayload {
    fn from(error: ScanError) -> Self {
        error.payload()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ErrorExtra {
    Size { size_bytes: u64 },
    Limit { limit: usize },
}

pub type ScanResult<T> = Result<T, ScanError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_file_confirmation_payload_is_machine_readable() {
        let err = ScanError::LargeFileNeedsConfirmation {
            size_bytes: 31_457_280,
            size_mb: 30.0,
        };
        let payload = err.payload();
        assert_eq!(payload.code, "LARGE_FILE_NEEDS_CONFIRMATION");
        assert_eq!(
            payload.extra,
            Some(ErrorExtra::Size {
                size_bytes: 31_457_280
            })
        );
        // 訊息可讀且含大小，但不含任何路徑資訊。
        assert!(payload.message.contains("30.0 MB"));
    }

    #[test]
    fn error_messages_never_leak_paths() {
        let errors = vec![
            ScanError::FileNotFound,
            ScanError::UnsupportedFormat("xls".into()),
            ScanError::Encrypted,
            ScanError::Corrupt {
                detail: "xml parse".into(),
            },
            ScanError::Internal {
                detail: "some detail".into(),
            },
        ];
        for err in errors {
            let msg = err.to_string();
            assert!(!msg.contains("C:\\"), "訊息不應含本機路徑：{msg}");
            assert!(!msg.contains("\\\\"), "訊息不應含 UNC 路徑：{msg}");
        }
    }
}
