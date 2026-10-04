//! Count allocations only on this test thread, excluding the test harness.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use witvoice_audio::format::{AudioFormat, Encoding, capture_to_mono, mono_to_render};
use witvoice_audio::stream::{SILENT, decode_packet};

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountingAllocator;
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.try_with(Cell::get).unwrap_or(false) {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.try_with(Cell::get).unwrap_or(false) {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn conversion_success_and_failure_paths_allocate_zero() {
    let formats = [
        Encoding::Pcm8,
        Encoding::Pcm16,
        Encoding::Pcm24,
        Encoding::Pcm32,
        Encoding::Float32,
    ];
    let input = [0.5; 16];
    let invalid = [f32::NAN; 16];
    let mut bytes = [0u8; 128];
    let mut mono = [0.0; 16];
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    for encoding in formats {
        for channels in 1..=2 {
            let format = AudioFormat::new(48_000, channels, encoding).unwrap();
            let output = &mut bytes[..input.len() * format.frame_bytes()];
            assert!(mono_to_render(format, &input, 1.0, output).is_ok());
            assert!(capture_to_mono(format, output, &mut mono).is_ok());
            assert!(mono_to_render(format, &invalid, 1.0, output).is_err());
            assert!(capture_to_mono(format, &[], &mut mono).is_err());
            assert!(decode_packet(format, 16, 0, Some(output), &mut mono).is_ok());
            assert!(decode_packet(format, 16, SILENT, None, &mut mono).is_ok());
            assert!(decode_packet(format, 17, 0, None, &mut mono).is_err());
        }
    }
    COUNTING.with(|enabled| enabled.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
}
