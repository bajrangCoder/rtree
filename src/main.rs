use clap::Parser;
use colored::*;
use glob::Pattern;
use std::fs;
use std::fs::File;
use std::io::{self, BufRead};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser)]
#[command(
    name = "rtree",
    version,
    author = "Raunak Raj <bajrangcoders@gmail.com>",
    about = "Tree clone"
)]
struct Opt {
    /// Path where to run rtree
    path: Option<PathBuf>,

    #[arg(short = 'd', long)]
    max_depth: Option<usize>,

    /// Include hidden files
    #[arg(short = 'h', long)]
    show_hidden: bool,

    /// Use parallelism (not implemented)
    #[arg(short, long)]
    parallel: bool,

    /// Pattern to ignore files/folders (separated by '|')
    #[arg(
        short,
        long
    )]
    ignore: Option<String>,

    /// Disable .gitignore file processing
    #[arg(short = 'g', long)]
    no_gitignore: bool,
}

#[derive(Default)]
struct Stats {
    directories: usize,
    files: usize,
}

fn main() {
    let mut opt = Opt::parse();
    if opt.path.is_none() {
        opt.path = Some(std::env::current_dir().unwrap());
    }
    let path = opt.path.as_ref().unwrap();

    let start = Instant::now();
    println!("{}", path.display());

    // Load ignore patterns
    let mut ignore_patterns: Vec<Pattern> = vec![];
    if let Some(ignore_str) = &opt.ignore {
        let patterns: Vec<&str> = ignore_str.split('|').collect();
        ignore_patterns.extend(patterns.iter().filter_map(|p| Pattern::new(p).ok()));
    }

    // Process .gitignore if not disabled
    if !opt.no_gitignore {
        if let Some(gitignore_patterns) = load_gitignore_patterns(path) {
            ignore_patterns.extend(gitignore_patterns);
        }
    }
    let stats = list_contents(path, &Vec::new(), &opt, &ignore_patterns);

    let duration = start.elapsed();

    println!("\n{} directories, {} files", stats.directories, stats.files);
    println!("Time taken: {:?}", duration);
}

// Load patterns from .gitignore file if present
fn load_gitignore_patterns(path: &Path) -> Option<Vec<Pattern>> {
    let gitignore_path = path.join(".gitignore");
    if gitignore_path.exists() {
        let file = File::open(gitignore_path).ok()?;
        let reader = io::BufReader::new(file);
        let patterns: Vec<Pattern> = reader
            .lines()
            .filter_map(Result::ok)
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
            .filter_map(|line| {
                // Handle patterns starting with "/"
                let trimmed_line = line.trim();
                if trimmed_line.starts_with('/') {
                    // Convert to an absolute pattern based on the given path
                    let absolute_pattern = path.join(trimmed_line.trim_start_matches('/'));
                    Pattern::new(absolute_pattern.to_str().unwrap()).ok()
                } else {
                    Pattern::new(trimmed_line).ok()
                }
            })
            .collect();
        return Some(patterns);
    }
    None
}

fn list_contents(
    dir: &Path,
    prefixes: &Vec<bool>,
    opt: &Opt,
    ignore_patterns: &[Pattern],
) -> Stats {
    let mut stats = Stats::default();

    if let Some(max_depth) = opt.max_depth {
        if prefixes.len() >= max_depth {
            return stats;
        }
    }

    if let Ok(entries_iter) = fs::read_dir(dir) {
        let mut entries: Vec<_> = entries_iter.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());

        // Filter entries after sorting
        let entries: Vec<_> = entries
            .into_iter()
            .filter(|entry| {
                let path = entry.path();
                let file_name = path.file_name().unwrap().to_string_lossy();

                if !opt.show_hidden && file_name.starts_with('.') {
                    return false;
                }

                // Check if the path matches any ignore pattern
                if ignore_patterns.iter().any(|pattern| {
                    // For absolute patterns, match against the full path
                    let path_str = path.to_string_lossy();
                    if pattern.as_str().starts_with('/') {
                        pattern.matches(&path_str)
                    } else {
                        pattern.matches(&file_name)
                    }
                }) {
                    return false;
                }

                true
            })
            .collect();

        let entries_len = entries.len();

        for (i, entry) in entries.into_iter().enumerate() {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_string_lossy();

            let is_last = i == entries_len - 1;

            // Build the prefix
            let mut prefix = String::new();
            for &last in prefixes.iter() {
                if last {
                    prefix.push_str("    ");
                } else {
                    prefix.push_str("│   ");
                }
            }
            if is_last {
                prefix.push_str("└── ");
            } else {
                prefix.push_str("├── ");
            }

            // Get metadata
            let metadata = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mut display = String::new();

            // Symbolic link
            if metadata.file_type().is_symlink() {
                let target = match fs::read_link(&path) {
                    Ok(t) => t,
                    Err(_) => PathBuf::from("unreadable"),
                };
                display = format!(
                    "{} -> {}",
                    file_name.cyan().italic(),
                    target.to_string_lossy().blue().italic()
                );

                println!("{}{}", prefix, display);
                stats.files += 1;

            // Directory
            } else if path.is_dir() {
                display = file_name.blue().bold().to_string();
                println!("{}{}", prefix, display);

                stats.directories += 1;
                let mut new_prefixes = prefixes.clone();
                new_prefixes.push(is_last);
                let sub_stats = list_contents(&path, &new_prefixes, opt, ignore_patterns);
                stats.directories += sub_stats.directories;
                stats.files += sub_stats.files;

            // Executable file
            } else if metadata.permissions().mode() & 0o111 != 0 {
                display = file_name.green().to_string();
                println!("{}{}", prefix, display);
                stats.files += 1;

            // Regular file (with language-based coloring)
            } else {
                display = match file_name.split('.').last() {
                    Some("svg") => file_name.magenta().to_string(),
                    Some("png") => file_name.magenta().to_string(),
                    Some("jpg") => file_name.magenta().to_string(),
                    Some("pdf") => file_name.red().to_string(),
                    Some("yaml") => file_name.yellow().to_string(),
                    Some("yml") => file_name.yellow().to_string(),
                    Some("zip") => file_name.red().to_string(),
                    Some("tar") => file_name.red().to_string(),
                    _ => file_name.to_string(),
                };

                println!("{}{}", prefix, display);
                stats.files += 1;
            }
        }
    }

    stats
}
