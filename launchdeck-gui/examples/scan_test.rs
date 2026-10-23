fn main() {
    launchdeck_core::init().ok();
    let apps = launchdeck_core::AppScanner::default().scan().unwrap_or_default();
    println!("=== 扫描到 {} 个应用 ===", apps.len());
    for (i, a) in apps.iter().take(15).enumerate() {
        println!("{:2}. {:<28} {:<40} {}",
            i + 1,
            a.name,
            a.bundle_id,
            launchdeck_core::format_bytes(a.size));
    }
    println!("...");
    let total: u64 = apps.iter().map(|a| a.size).sum();
    println!("总计大小: {}", launchdeck_core::format_bytes(total));
}
