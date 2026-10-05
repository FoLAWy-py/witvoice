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

#[test]
fn queued_governor_success_rejections_expiry_underflow_and_tickets_allocate_zero() {
    use witvoice_audio::realtime::*;
    let binding = Binding::new(5, 9).unwrap();
    let mut gate = OutputGate::new(binding).unwrap();
    gate.arm().unwrap();
    let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    let a = ProcessedBlock::from_model_result(binding, 0, 0, 60_000_000, &[0.7; 480]).unwrap();
    let b = ProcessedBlock::from_model_result(binding, 480, 0, 60_000_000, &[0.7; 480]).unwrap();
    let mut out = [0.0; 480];
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|flag| flag.set(true));
    writer.push(a, 0).unwrap();
    writer.push(b, 0).unwrap();
    assert_eq!(writer.push(a, 0), Err(BlockError::Stale));
    let ticket = gate.begin_commit().unwrap();
    sink.render(&mut out, 0).unwrap();
    assert!(sink.commit_valid(1));
    assert_eq!(sink.render(&mut out, 60_000_000), Err(BlockError::Expired));
    assert_eq!(out, [0.0; 480]);
    assert!(!ticket.is_live());
    assert!(!gate.ack_ready());
    drop(ticket);
    assert!(gate.ack_ready());
    assert!(gate.begin_commit().is_none());
    assert_eq!(sink.render(&mut out, 60_000_001), Err(BlockError::Muted));
    assert!(ProcessedBlock::from_model_result(binding, 960, 0, 1, &[f32::NAN; 480]).is_err());
    COUNTING.with(|flag| flag.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
    let mut capture = Ring::<CaptureBlock>::new(4).unwrap();
    let (mut writer, mut worker) =
        capture_endpoints(&mut capture, binding, QueueConfig::capture()).unwrap();
    let c = CaptureBlock::from_capture(binding, 0, 0, 20_000_000, &[0.5; 480]).unwrap();
    let mut out = [0.9; 480];
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|flag| flag.set(true));
    writer.push(c, 0).unwrap();
    worker.read(&mut out, 1).unwrap();
    assert_eq!(worker.read(&mut out, 2), Err(BlockError::Underflow));
    assert_eq!(out, [0.0; 480]);
    COUNTING.with(|flag| flag.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
}
