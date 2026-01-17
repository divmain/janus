use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

/// Helper struct to run janus commands in an isolated temp directory
pub struct JanusTest {
    pub temp_dir: TempDir,
    binary_path: String,
}

impl JanusTest {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Find the binary - check both debug and release
        let binary_path = if cfg!(debug_assertions) {
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus")
        } else {
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/release/janus")
        };

        // If the above doesn't exist, try the alternative
        let binary_path = if std::path::Path::new(binary_path).exists() {
            binary_path.to_string()
        } else {
            // Fallback to debug
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus").to_string()
        };

        JanusTest {
            temp_dir,
            binary_path,
        }
    }

    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.binary_path)
            .args(args)
            .current_dir(self.temp_dir.path())
            .output()
            .expect("Failed to execute janus command")
    }

    pub fn run_success(&self, args: &[&str]) -> String {
        let output = self.run(args);
        if !output.status.success() {
            panic!(
                "Command {:?} failed with status {:?}\nstdout: {}\nstderr: {}",
                args,
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    pub fn run_failure(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            !output.status.success(),
            "Expected command {:?} to fail, but it succeeded",
            args
        );
        String::from_utf8_lossy(&output.stderr).to_string()
    }

    pub fn read_ticket(&self, id: &str) -> String {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("items")
            .join(format!("{}.md", id));
        fs::read_to_string(path).expect("Failed to read ticket file")
    }

    pub fn ticket_exists(&self, id: &str) -> bool {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("items")
            .join(format!("{}.md", id));
        path.exists()
    }

    pub fn write_ticket(&self, id: &str, content: &str) {
        let dir = self.temp_dir.path().join(".janus").join("items");
        fs::create_dir_all(&dir).expect("Failed to create .janus/items directory");
        let path = dir.join(format!("{}.md", id));
        fs::write(path, content).expect("Failed to write ticket file");
    }

    pub fn read_plan(&self, id: &str) -> String {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("plans")
            .join(format!("{}.md", id));
        fs::read_to_string(path).expect("Failed to read plan file")
    }

    pub fn plan_exists(&self, id: &str) -> bool {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("plans")
            .join(format!("{}.md", id));
        path.exists()
    }

    pub fn write_plan(&self, id: &str, content: &str) {
        let dir = self.temp_dir.path().join(".janus").join("plans");
        fs::create_dir_all(&dir).expect("Failed to create .janus/plans directory");
        let path = dir.join(format!("{}.md", id));
        fs::write(path, content).expect("Failed to write plan file");
    }

    pub fn write_config(&self, content: &str) {
        let dir = self.temp_dir.path().join(".janus");
        fs::create_dir_all(&dir).expect("Failed to create .janus directory");
        let path = dir.join("config.yaml");
        fs::write(path, content).expect("Failed to write config file");
    }

    pub fn write_hook_script(&self, name: &str, content: &str) {
        let dir = self.temp_dir.path().join(".janus").join("hooks");
        fs::create_dir_all(&dir).expect("Failed to create .janus/hooks directory");
        let path = dir.join(name);
        fs::write(&path, content).expect("Failed to write hook script");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .expect("Failed to set hook script permissions");
        }
    }

    #[allow(dead_code)]
    pub fn read_file(&self, relative_path: &str) -> Option<String> {
        let path = self.temp_dir.path().join(relative_path);
        fs::read_to_string(path).ok()
    }
}
