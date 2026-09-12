#![allow(dead_code)]

use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{CaptureRecord, CapturedFormat, IdentityKey, Timestamp};

pub use duplicata_core::utf16le;

pub fn text_record(tag: u8, text: &str, ts_ms: u64) -> CaptureRecord {
    let unicode = utf16le(text);
    let formats = vec![
        CapturedFormat {
            format_id: 1,
            format_name: None,
            bytes: text.as_bytes().to_vec(),
        },
        CapturedFormat {
            format_id: 13,
            format_name: None,
            bytes: unicode.clone(),
        },
    ];
    let total = formats.iter().map(|f| f.bytes.len() as u64).sum();
    CaptureRecord {
        identity: IdentityKey([tag; 32]),
        canonical: CanonicalSelection {
            format_id: 13,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: unicode.len() as u64,
        },
        captured_at: Timestamp::from_millis(ts_ms),
        total_bytes: total,
        preview: Some(text.to_string()),
        thumbnail: None,
        has_text: true,
        formats,
    }
}

pub fn image_record(
    tag: u8,
    dib_bytes: Vec<u8>,
    thumbnail: Option<Vec<u8>>,
    ts_ms: u64,
) -> CaptureRecord {
    let formats = vec![CapturedFormat {
        format_id: 8,
        format_name: None,
        bytes: dib_bytes,
    }];
    let total = formats.iter().map(|f| f.bytes.len() as u64).sum();
    CaptureRecord {
        identity: IdentityKey([tag; 32]),
        canonical: CanonicalSelection {
            format_id: 8,
            format_name: None,
            kind: CanonicalKind::Dib,
            byte_len: total,
        },
        captured_at: Timestamp::from_millis(ts_ms),
        total_bytes: total,
        preview: None,
        thumbnail,
        has_text: false,
        formats,
    }
}

pub fn cf_html(fragment: &str, source_url: &str) -> Vec<u8> {
    let pre = "<html>\r\n<body>\r\n<!--StartFragment-->";
    let post = "<!--EndFragment-->\r\n</body>\r\n</html>";

    let header_zeroed = header(0, 0, 0, 0, source_url);
    let header_len = header_zeroed.len();

    let body = format!("{pre}{fragment}{post}");
    let start_html = header_len;
    let end_html = header_len + body.len();
    let start_fragment = header_len + pre.len();
    let end_fragment = start_fragment + fragment.len();

    let header = header(
        start_html,
        end_html,
        start_fragment,
        end_fragment,
        source_url,
    );
    debug_assert_eq!(header.len(), header_len, "campos de largura fixa");
    format!("{header}{body}").into_bytes()
}

fn header(sh: usize, eh: usize, sf: usize, ef: usize, url: &str) -> String {
    format!(
        "Version:0.9\r\n\
         StartHTML:{sh:010}\r\n\
         EndHTML:{eh:010}\r\n\
         StartFragment:{sf:010}\r\n\
         EndFragment:{ef:010}\r\n\
         SourceURL:{url}\r\n"
    )
}
