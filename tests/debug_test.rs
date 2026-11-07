use gba_emu::gba::Gba;

#[test]
fn test_emulator_initialization() {
    let mut gba = Box::new(Gba::new());
    assert!(gba.reset());
    gba.run_frame();
    assert_eq!(gba.bus.ppu.framebuffer.len(), 240 * 160);
}
