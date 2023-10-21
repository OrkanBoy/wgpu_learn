const KEY_CODE_COUNT: usize = 128;
type KeysBitmask = u128;

pub struct InputState {
    pub keys_pressed_bitmask: KeysBitmask,
    pub previous_keys_pressed_bitmask: KeysBitmask,
    pub delta_mouse_pos: [f32; 2],
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_pressed_bitmask: 0b0,
            previous_keys_pressed_bitmask: 0b0,
            delta_mouse_pos: [0.0, 0.0],
        }
    }

    #[inline(always)]
    pub fn is_key_pressed(&mut self, key_code: winit::event::VirtualKeyCode) -> bool {
        let key_code_usize = key_code as usize;
        assert!(
            key_code_usize < KEY_CODE_COUNT,
            "key_code: {:?} not supported",
            key_code
        );
        self.keys_pressed_bitmask & (1 << key_code_usize) != 0
    }

    #[inline(always)]
    pub fn was_key_pressed(&mut self, key_code: winit::event::VirtualKeyCode) -> bool {
        let key_code_usize = key_code as usize;
        assert!(
            key_code_usize < KEY_CODE_COUNT,
            "key_code: {:?} not supported",
            key_code
        );
        self.previous_keys_pressed_bitmask & (1 << key_code_usize) != 0
    }

    #[inline(always)]
    pub fn set_key_pressed(&mut self, key_code: winit::event::VirtualKeyCode, pressed: bool) {
        let key_code_usize = key_code as usize;
        assert!(
            key_code_usize < KEY_CODE_COUNT,
            "key_code: {:?} not supported",
            key_code
        );
        self.keys_pressed_bitmask &= !(1 << key_code_usize);
        self.keys_pressed_bitmask |= (pressed as KeysBitmask) << key_code_usize;
    }
}