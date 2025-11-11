use latch_env::memory::Memory;

fn main() {
    let start_mem_collect = std::time::Instant::now();
    let memory = Memory::detect();
    let mem_collect_duration = start_mem_collect.elapsed().as_micros() as u64;
    println!("Detected memory configuration:");
    println!("  Cache line size: {} bytes", memory.cache_line);
    println!("  L1 cache size: {} bytes", memory.l1);
    println!("  L2 cache size: {} bytes", memory.l2);
    println!("  L3 cache size: {} bytes", memory.l3);
    println!("  Total RAM: {} bytes", memory.total_ram);
    println!("Memory stats collected in {} microseconds", mem_collect_duration);

    let start_mem_collect = std::time::Instant::now();
    let _memory = Memory::detect();
    let mem_collect_duration = start_mem_collect.elapsed().as_micros() as u64;
    println!("Second call collected in {} microseconds (cached)", mem_collect_duration);
}