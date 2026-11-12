use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A filesystem entry with aggregated size (dirs include children).
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Entry>,
}

/// Live counters while a tree is being walked.
#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

/// Aggregated totals for one file extension under a tree.
#[derive(Debug, Clone)]
pub struct ExtStat {
    pub ext: String,
    pub size: u64,
    pub count: u64,
}

impl Entry {
    pub fn scan(root: &Path) -> io::Result<Self> {
        Ok(Self::scan_with_progress(root, |_| {})?.0)
    }

    pub fn scan_with_progress(
        root: &Path,
        mut on_progress: impl FnMut(&ScanStats),
    ) -> io::Result<(Self, ScanStats)> {
        let mut stats = ScanStats::default();
        let entry = scan_path(root, &mut stats, &mut on_progress)?;
        on_progress(&stats);
        Ok((entry, stats))
    }

    /// Sum file sizes by extension within this subtree (directories excluded).
    pub fn extension_stats(&self) -> Vec<ExtStat> {
        let mut map = std::collections::HashMap::<String, (u64, u64)>::new();
        collect_extensions(self, &mut map);
        let mut stats: Vec<ExtStat> = map
            .into_iter()
            .map(|(ext, (size, count))| ExtStat { ext, size, count })
            .collect();
        stats.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.ext.cmp(&b.ext)));
        stats
    }

    /// Files under this subtree matching `ext` (e.g. `.qcow2` or `(no extension)`), largest first.
    pub fn files_with_extension(&self, ext: &str) -> Vec<ExtFile> {
        let mut files = Vec::new();
        collect_files_with_extension(self, ext, &mut files);
        files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
        files
    }
}

/// One file matched while drilling into an extension.
#[derive(Debug, Clone)]
pub struct ExtFile {
    pub path: PathBuf,
    pub size: u64,
}

fn collect_extensions(entry: &Entry, map: &mut std::collections::HashMap<String, (u64, u64)>) {
    if entry.is_dir {
        for child in &entry.children {
            collect_extensions(child, map);
        }
        return;
    }
    let ext = extension_label(&entry.name);
    let slot = map.entry(ext).or_insert((0, 0));
    slot.0 = slot.0.saturating_add(entry.size);
    slot.1 = slot.1.saturating_add(1);
}

fn extension_label(name: &str) -> String {
    // Dotfiles like ".gitignore" have no extension; "archive.tar.gz" -> ".gz"
    let Some(dot) = name.rfind('.') else {
        return "(no extension)".to_string();
    };
    if dot == 0 || dot + 1 >= name.len() {
        return "(no extension)".to_string();
    }
    format!(".{}", name[dot + 1..].to_ascii_lowercase())
}

fn collect_files_with_extension(entry: &Entry, ext: &str, out: &mut Vec<ExtFile>) {
    if entry.is_dir {
        for child in &entry.children {
            collect_files_with_extension(child, ext, out);
        }
        return;
    }
    if extension_label(&entry.name) == ext {
        out.push(ExtFile {
            path: entry.path.clone(),
            size: entry.size,
        });
    }
}

fn scan_path(
    path: &Path,
    stats: &mut ScanStats,
    on_progress: &mut impl FnMut(&ScanStats),
) -> io::Result<Entry> {
    let meta = fs::symlink_metadata(path)?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    if meta.file_type().is_symlink() {
        let size = meta.len();
        stats.files = stats.files.saturating_add(1);
        stats.bytes = stats.bytes.saturating_add(size);
        on_progress(stats);
        return Ok(Entry {
            name,
            path: path.to_path_buf(),
            size,
            is_dir: false,
            children: Vec::new(),
        });
    }

    if meta.is_file() {
        let size = meta.len();
        stats.files = stats.files.saturating_add(1);
        stats.bytes = stats.bytes.saturating_add(size);
        on_progress(stats);
        return Ok(Entry {
            name,
            path: path.to_path_buf(),
            size,
            is_dir: false,
            children: Vec::new(),
        });
    }

    if !meta.is_dir() {
        let size = meta.len();
        stats.files = stats.files.saturating_add(1);
        stats.bytes = stats.bytes.saturating_add(size);
        on_progress(stats);
        return Ok(Entry {
            name,
            path: path.to_path_buf(),
            size,
            is_dir: false,
            children: Vec::new(),
        });
    }

    stats.dirs = stats.dirs.saturating_add(1);
    on_progress(stats);

    let mut children = Vec::new();
    let mut total = 0u64;

    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            return Ok(Entry {
                name,
                path: path.to_path_buf(),
                size: 0,
                is_dir: true,
                children,
            });
        }
    };

    for item in rd.flatten() {
        let child_path = item.path();
        match scan_path(&child_path, stats, on_progress) {
            Ok(child) => {
                total = total.saturating_add(child.size);
                children.push(child);
            }
            Err(_) => continue,
        }
    }

    children.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));

    Ok(Entry {
        name,
        path: path.to_path_buf(),
        size: total,
        is_dir: true,
        children,
    })
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1000), "1.00 KB");
        assert_eq!(human_size(1_500_000), "1.50 MB");
    }

    #[test]
    fn scan_sums_nested_files() {
        let dir = std::env::temp_dir().join(format!("duviz-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::File::create(dir.join("a.txt"))
            .unwrap()
            .write_all(&[0u8; 100])
            .unwrap();
        fs::File::create(dir.join("sub").join("b.txt"))
            .unwrap()
            .write_all(&[0u8; 50])
            .unwrap();

        let tree = Entry::scan(&dir).unwrap();
        assert!(tree.is_dir);
        assert_eq!(tree.size, 150);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extension_stats_groups_files() {
        let dir = std::env::temp_dir().join(format!("duviz-ext-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::File::create(dir.join("a.TXT"))
            .unwrap()
            .write_all(&[0u8; 100])
            .unwrap();
        fs::File::create(dir.join("sub").join("b.txt"))
            .unwrap()
            .write_all(&[0u8; 50])
            .unwrap();
        fs::File::create(dir.join("c.rs"))
            .unwrap()
            .write_all(&[0u8; 20])
            .unwrap();
        fs::File::create(dir.join("Makefile"))
            .unwrap()
            .write_all(&[0u8; 10])
            .unwrap();

        let tree = Entry::scan(&dir).unwrap();
        let stats = tree.extension_stats();
        assert_eq!(stats[0].ext, ".txt");
        assert_eq!(stats[0].size, 150);
        assert_eq!(stats[0].count, 2);
        assert_eq!(stats[1].ext, ".rs");
        assert_eq!(stats[1].size, 20);
        assert_eq!(stats[1].count, 1);
        assert_eq!(stats[2].ext, "(no extension)");
        assert_eq!(stats[2].size, 10);

        let txt = tree.files_with_extension(".txt");
        assert_eq!(txt.len(), 2);
        assert_eq!(txt[0].size, 100);

        fs::remove_dir_all(&dir).unwrap();
    }
}
