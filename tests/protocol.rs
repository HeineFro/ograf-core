use ograf_core::models::is_valid_renderer_name;

#[test]
fn dot_segments_are_not_renderer_ids() {
    assert!(!is_valid_renderer_name("."));
    assert!(!is_valid_renderer_name(".."));
    assert!(is_valid_renderer_name("..."));
    assert!(is_valid_renderer_name("a.b"));
    assert!(is_valid_renderer_name("connect"));
}
