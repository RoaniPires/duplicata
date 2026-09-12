use std::sync::mpsc;

use duplicata_core::capture::{CanonicalKind, CanonicalSelection, CapturedFormat, Timestamp};
use duplicata_core::queue::{ByteBudgetQueue, CaptureQueue};
use duplicata_core::work_item::WorkItem;
use duplicata_core::RawCapture;

fn raw(tag: u8) -> RawCapture {
    let formats = vec![CapturedFormat {
        format_id: 13,
        format_name: None,
        bytes: vec![tag],
    }];
    let canonical = CanonicalSelection {
        format_id: 13,
        format_name: None,
        kind: CanonicalKind::UnicodeText,
        byte_len: 1,
    };
    RawCapture::new(formats, canonical, Timestamp::from_millis(tag as u64))
}

#[test]
fn recreate_database_interleaved_with_captures_preserves_every_item_and_never_blocks_pop() {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let (tx, rx) = mpsc::channel();

    queue.push(WorkItem::Capture(raw(1)), 1);
    queue.push(WorkItem::RecreateDatabase { done: tx }, 0);
    queue.push(WorkItem::Capture(raw(2)), 1);
    queue.close();

    let mut capture_tags = Vec::new();
    let mut recreate_count = 0;
    while let Some(item) = queue.pop() {
        match item {
            WorkItem::Capture(raw) => capture_tags.push(raw.formats[0].bytes[0]),
            WorkItem::RecoverCapture(_)
            | WorkItem::ApplySettings { .. }
            | WorkItem::SetPinned { .. }
            | WorkItem::DeleteAll { .. }
            | WorkItem::ToggleEncryption { .. } => {
                unreachable!("este teste só enfileira Capture e RecreateDatabase")
            }
            WorkItem::RecreateDatabase { done } => {
                recreate_count += 1;
                done.send(Ok(())).expect("receptor ainda vivo");
            }
        }
    }
    assert!(queue.pop().is_none(), "fila fechada e vazia devolve None");

    assert_eq!(
        capture_tags,
        vec![1, 2],
        "nenhuma captura perdida, ordem preservada"
    );
    assert_eq!(
        recreate_count, 1,
        "RecreateDatabase não perdido nem duplicado"
    );
    assert!(
        rx.recv().expect("done enviado pela worker").is_ok(),
        "resultado chega a quem pediu a recriação"
    );
}
