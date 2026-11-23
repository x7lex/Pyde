//! The type-checking engine: owns the database and the file being edited.

mod check;
mod db;
mod docs;
mod hover;
mod signature;

use pyde_analysis::protocol::PackageFiles;
use ruff_db::files::File;
use ruff_db::system::{DbWithWritableSystem as _, SystemPathBuf};

use db::{SITE_PACKAGES, TyDb};

/// The checker for one file at a time: check it, then ask about it.
pub struct TyEngine {
    db: TyDb,
    current_file: Option<File>,
    current_path: Option<SystemPathBuf>,
}

impl Default for TyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TyEngine {
    pub fn new() -> Self {
        Self {
            db: TyDb::new(),
            current_file: None,
            current_path: None,
        }
    }

    /// Makes installed packages importable. Call `check` again afterwards.
    pub fn add_packages(&mut self, packages: &[PackageFiles]) {
        for package in packages {
            for (path, text) in &package.files {
                if path.split('/').any(|part| part == ".." || part.is_empty()) {
                    continue;
                }
                let _ = self
                    .db
                    .write_file(SystemPathBuf::from(format!("{SITE_PACKAGES}/{path}")), text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, files: &[(&str, &str)]) -> PackageFiles {
        PackageFiles {
            name: name.into(),
            files: files
                .iter()
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect(),
        }
    }

    #[test]
    fn installed_packages_become_importable() {
        let mut engine = TyEngine::new();
        let code = "import fakelib\nx: str = fakelib.answer()\n";
        let (before, _) = engine.check("main.py", code);
        assert!(
            before.iter().any(|d| d.rule == "import-not-found"),
            "{before:?}"
        );

        engine.add_packages(&[pkg(
            "fakelib",
            &[
                ("fakelib/__init__.pyi", "def answer() -> int: ...\n"),
                ("../evil.py", "x = 1"),
            ],
        )]);
        let (after, _) = engine.check("main.py", code);
        assert!(
            !after.iter().any(|d| d.rule == "import-not-found"),
            "{after:?}"
        );
        assert!(
            after.iter().any(|d| d.rule == "invalid-assignment"),
            "{after:?}"
        );
    }
}
