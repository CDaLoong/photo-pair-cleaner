#![allow(dead_code)]

#[path = "../src/watermark_model.rs"]
mod watermark_model;
#[path = "../src/watermark_templates.rs"]
mod watermark_templates;

use watermark_model::default_template;

#[test]
fn exposes_exactly_six_valid_immutable_builtins() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("watermark-templates.json");
    let entries = watermark_templates::list_templates(&database).unwrap();
    let builtins = entries
        .iter()
        .filter(|entry| entry.built_in)
        .collect::<Vec<_>>();
    assert_eq!(builtins.len(), 6);
    assert_eq!(builtins[0].template.id, "minimal-signature");
    assert_eq!(builtins[5].template.id, "transparent-logo");
    assert!(
        builtins
            .iter()
            .all(|entry| entry.template.variants.len() == 3)
    );
    assert!(watermark_templates::delete_template(&database, "minimal-signature").is_err());
}

#[test]
fn local_templates_save_copy_rename_and_delete_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("watermark-templates.json");
    let builtin = watermark_templates::built_in_templates().unwrap().remove(0);
    assert!(watermark_templates::save_template(&database, builtin.clone(), false).is_err());
    let saved = watermark_templates::save_template(&database, builtin, true).unwrap();
    assert!(!saved.built_in);
    assert!(saved.template.id.starts_with("local-"));
    let mut renamed = saved.template.clone();
    renamed.name = "我的署名".into();
    let renamed = watermark_templates::save_template(&database, renamed, false).unwrap();
    assert_eq!(renamed.template.name, "我的署名");
    assert!(database.is_file());
    watermark_templates::delete_template(&database, &renamed.template.id).unwrap();
    assert!(
        watermark_templates::list_templates(&database)
            .unwrap()
            .iter()
            .all(|entry| entry.template.id != renamed.template.id)
    );
}

#[test]
fn portable_json_import_creates_a_new_local_id_and_rejects_future_versions() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("watermark-templates.json");
    let export = temp.path().join("portable.framepair-watermark.json");
    let mut template = default_template("portable", "便携模板");
    template.shared.palette.push("#123456".into());
    watermark_templates::export_template(&export, &template).unwrap();
    let imported = watermark_templates::import_template(&database, &export).unwrap();
    assert!(imported.template.id.starts_with("local-"));
    assert_ne!(imported.template.id, "portable");
    assert_eq!(imported.template.name, "便携模板");

    let future = temp.path().join("future.json");
    std::fs::write(
        &future,
        serde_json::json!({ "schemaVersion": 99, "template": template }).to_string(),
    )
    .unwrap();
    assert!(watermark_templates::import_template(&database, &future).is_err());
}

#[test]
fn portable_validation_rejects_tampered_embedded_resources() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("watermark-templates.json");
    let export = temp.path().join("logo.json");
    let mut template = watermark_templates::built_in_templates().unwrap().remove(5);
    template.resources.values_mut().next().unwrap().sha256 = "0".repeat(64);
    std::fs::write(
        &export,
        serde_json::json!({ "schemaVersion": 1, "template": template }).to_string(),
    )
    .unwrap();
    assert!(watermark_templates::import_template(&database, &export).is_err());
}
