//! End-to-end project lifecycle test against the public API only:
//! create → hand-edit with a normal text editor → open → write narrative
//! content → rebuild index → reject an unknown schema without rewriting.

use narra_core::{
    DEFAULT_SCREENPLAY, MANIFEST_FILE, ProjectType, create_project, open_project, parse_manifest,
    serialize_manifest,
};
use std::fs;
use std::path::PathBuf;

fn temp_root() -> PathBuf {
    tempfile::tempdir().expect("tempdir").keep()
}

#[test]
fn full_lifecycle_stays_portable_and_strict() {
    let root = temp_root();

    // 1. Create from the app.
    let _project =
        create_project(&root, "El proyecto", ProjectType::Feature, "es").expect("create");
    let manifest_text = fs::read_to_string(root.join(MANIFEST_FILE)).expect("manifest");
    assert!(manifest_text.starts_with("schema_version: 1\n"));

    // 2. User edits the manifest by hand in a text editor (adds a language).
    let edited =
        manifest_text.replace("additional_languages: []", "additional_languages: [\"en\"]");
    fs::write(root.join(MANIFEST_FILE), &edited).expect("hand edit");
    let reopened = open_project(&root).expect("reopen after hand edit");
    assert_eq!(
        reopened.manifest().additional_languages,
        vec!["en".to_string()]
    );

    // 3. Write narrative content and rebuild the disposable index.
    reopened
        .write_document(
            DEFAULT_SCREENPLAY,
            "INT. OFICINA - DÍA\n\nElla teclea.\n\nEXT. AZOTEA - NOCHE\n",
        )
        .expect("write screenplay");
    reopened
        .write_document("bible/characters/marta.md", "# Marta\n")
        .expect("write character");
    let index = reopened.rebuild_index().expect("index");
    assert_eq!(index.scenes.len(), 2);

    // 4. Everything on disk is plain text a Git client can diff.
    let status_paths = [
        MANIFEST_FILE,
        DEFAULT_SCREENPLAY,
        "bible/characters/marta.md",
        "notes/decisions.md",
    ];
    for rel in status_paths {
        let path = root.join(rel);
        assert!(path.is_file(), "missing {rel}");
        assert!(String::from_utf8(fs::read(&path).expect("read")).is_ok());
    }

    // 5. A future/unknown schema fails explicitly and never rewrites the file.
    let before = fs::read_to_string(root.join(MANIFEST_FILE)).expect("read manifest");
    let future = before.replace("schema_version: 1", "schema_version: 7");
    fs::write(root.join(MANIFEST_FILE), &future).expect("write future schema");
    let error = open_project(&root).expect_err("schema 7 must be rejected");
    assert!(
        error.to_string().contains("unsupported"),
        "unexpected message: {error}"
    );
    let after = fs::read_to_string(root.join(MANIFEST_FILE)).expect("read back");
    assert_eq!(after, future, "reject must not rewrite the manifest");

    // 6. The round trip through parse/serialize keeps user data.
    let parsed = parse_manifest(&after).expect_err("parse must also reject v7");
    assert!(parsed.to_string().contains("schema_version 7"));
    let restored = parse_manifest(&before).expect("parse v1");
    assert_eq!(serialize_manifest(&restored).expect("serialize"), before);
}
