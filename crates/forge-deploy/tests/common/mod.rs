#![allow(dead_code)]

pub mod ftp;
pub mod sftp;

use std::path::Path;

/// A small Laravel-like project.
pub fn project(root: &Path) {
    let files: &[(&str, &str)] = &[
        ("composer.json", "{\"name\":\"acme/shop\"}"),
        ("artisan", "#!/usr/bin/env php"),
        ("app/Models/User.php", "<?php class User {}"),
        ("app/Models/Order.php", "<?php class Order {}"),
        (
            "public/index.php",
            "<?php require '../vendor/autoload.php';",
        ),
        ("public/css/app.css", "body { color: red; }"),
        ("storage/logs/laravel.log", "local log"),
        (".git/HEAD", "ref: refs/heads/main"),
        ("app/.DS_Store", "junk"),
    ];
    for (path, text) in files {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
}

/// Set a file's modification time.
pub fn touch(path: &Path, secs: i64) {
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(secs, 0)).unwrap();
}
