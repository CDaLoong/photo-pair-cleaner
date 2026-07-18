#[path = "../src/watermark_model.rs"]
mod watermark_model;

use watermark_model::{default_template, validate_template};

#[test]
fn default_template_contains_three_valid_variants() {
    let template = default_template("template-1", "未命名模板");
    assert!(validate_template(&template).is_ok());
    assert_eq!(template.schema_version, 1);
    assert_eq!(template.variants.len(), 3);
}

#[test]
fn validation_rejects_out_of_range_normalized_values() {
    let mut template = default_template("template-1", "无效模板");
    template.variants.get_mut("landscape").unwrap().frame.left = 1.5;
    assert!(validate_template(&template).unwrap_err().contains("边框"));
}
