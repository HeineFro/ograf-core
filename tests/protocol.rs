use ograf_core::models::{is_valid_renderer_name, RendererMessage};

fn hello_id(json: &str) -> String {
    match serde_json::from_str::<RendererMessage>(json).unwrap() {
        RendererMessage::Hello { renderer_id, .. } => renderer_id,
        other => panic!("expected hello, got {other:?}"),
    }
}

#[test]
fn hello_carries_the_renderer_id() {
    let id = hello_id(r#"{"type":"hello","rendererId":"studio1-ch1-l10","renderTarget":{}}"#);
    assert_eq!(id, "studio1-ch1-l10");
}

#[test]
fn hello_still_accepts_the_old_name_field() {
    let id = hello_id(r#"{"type":"hello","name":"studio1-ch1-l10","renderTarget":{}}"#);
    assert_eq!(id, "studio1-ch1-l10");
}

#[test]
fn dot_segments_are_not_renderer_ids() {
    assert!(!is_valid_renderer_name("."));
    assert!(!is_valid_renderer_name(".."));
    assert!(is_valid_renderer_name("..."));
    assert!(is_valid_renderer_name("a.b"));
    assert!(is_valid_renderer_name("connect"));
}
