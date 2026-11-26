//! The checker's database: Ty's storage over an in-memory file system, set up
//! for a web page (no disk, no environment, typeshed only).

use std::sync::Arc;

use ruff_db::Db as SourceDb;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, Files};
use ruff_db::system::{DbWithTestSystem, System, SystemPathBuf, TestSystem};
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashSet;
use ty_module_resolver::{Db as ModuleResolverDb, FallibleStrategy, SearchPathSettings};
use ty_python_core::program::ProgramSettings;
use ty_python_core::{Db as CoreDb, ProgramFile, TestProgramDb};
use ty_python_semantic::lint::RuleSelection;
use ty_python_semantic::{
    AnalysisSettings, Db as SemanticDb, ReturnTypeInference, check_file_unwrap,
    default_lint_registry,
};

/// Where installed packages live in the checker's in-memory file system.
pub const SITE_PACKAGES: &str = "/site-packages";

#[salsa::db]
#[derive(Clone)]
pub struct TyDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: TestSystem,
    vendored: VendoredFileSystem,
    rule_selection: Arc<RuleSelection>,
    analysis_settings: Arc<AnalysisSettings>,
    pub(super) open_files: FxHashSet<File>,
    program_settings: ProgramSettings,
}

impl TyDb {
    pub(super) fn new() -> Self {
        let vendored = ty_vendored::file_system().clone();
        let system = TestSystem::default();
        let src_roots = vec![SystemPathBuf::from("/")];
        let mut search_path_settings = SearchPathSettings::new(src_roots);
        let site_packages = SystemPathBuf::from(SITE_PACKAGES);
        system
            .memory_file_system()
            .create_directory_all(&site_packages)
            .expect("memory fs is writable");
        search_path_settings.site_packages_paths = vec![site_packages];
        let python_version = PythonVersion::default();
        let python_version_source = ty_site_packages::PythonVersionWithSource {
            version: python_version,
            source: ty_site_packages::PythonVersionSource::default(),
        };

        let search_paths = search_path_settings
            .to_search_paths(&system, &vendored, &FallibleStrategy)
            .expect("default search paths are valid");

        let analysis_settings = AnalysisSettings {
            return_type_inference: ReturnTypeInference::Always,
            ..Default::default()
        };

        let db = Self {
            storage: salsa::Storage::default(),
            system,
            vendored,
            files: Files::default(),
            rule_selection: Arc::new(RuleSelection::from_registry(default_lint_registry())),
            analysis_settings: Arc::new(analysis_settings),
            open_files: FxHashSet::default(),
            program_settings: ProgramSettings {
                python_environment: Ok(None),
                python_version: python_version_source,
                python_platform: ty_python_core::platform::PythonPlatform::default(),
                search_paths,
            },
        };

        db.program_settings
            .search_paths
            .try_register_static_roots(&db);
        db
    }
}

impl Default for TyDb {
    fn default() -> Self {
        Self::new()
    }
}

impl DbWithTestSystem for TyDb {
    fn test_system(&self) -> &TestSystem {
        &self.system
    }

    fn test_system_mut(&mut self) -> &mut TestSystem {
        &mut self.system
    }
}

#[salsa::db]
impl salsa::Database for TyDb {}

#[salsa::db]
impl SourceDb for TyDb {
    fn vendored(&self) -> &VendoredFileSystem {
        &self.vendored
    }

    fn system(&self) -> &dyn System {
        &self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }
}

#[salsa::db]
impl ModuleResolverDb for TyDb {}

#[salsa::db]
impl ty_python_core::Db for TyDb {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }
}

#[salsa::db]
impl TestProgramDb for TyDb {
    fn program_settings(&self) -> &ProgramSettings {
        &self.program_settings
    }
}

#[salsa::db]
impl SemanticDb for TyDb {
    fn check_file(&self, file: File) -> Vec<Diagnostic> {
        if !CoreDb::should_check_file(self, file) {
            return Vec::new();
        }
        check_file_unwrap(self, self.program_file(file))
    }

    fn program_file(&self, file: File) -> ProgramFile<'_> {
        self.program().program_file(self, file)
    }

    fn python_version_with_source(
        &self,
        _file: File,
    ) -> &ty_site_packages::PythonVersionWithSource {
        &self.program_settings.python_version
    }

    fn rule_selection(&self, _file: File) -> &RuleSelection {
        &self.rule_selection
    }

    fn lint_registry(&self) -> &ty_python_semantic::lint::LintRegistry {
        default_lint_registry()
    }

    fn analysis_settings(&self, _file: File) -> &AnalysisSettings {
        &self.analysis_settings
    }

    fn dependency_metadata(
        &self,
        _file: File,
    ) -> Option<&ty_python_semantic::dependency::DependencyMetadata> {
        None
    }

    fn verbose(&self) -> bool {
        false
    }

    fn is_open_file(&self, file: File) -> bool {
        self.open_files.contains(&file)
    }

    fn dyn_clone(&self) -> Box<dyn SemanticDb> {
        Box::new(self.clone())
    }
}
