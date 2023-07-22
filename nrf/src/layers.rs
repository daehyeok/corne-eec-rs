use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::blocking_mutex::Mutex;

pub const COLS: usize = 12;
pub const ROWS: usize = 5;
pub const N_LAYERS: usize = 1;

pub type Layers = keyberon::layout::Layers<COLS, ROWS, N_LAYERS>;
#[allow(dead_code)]
pub type Layout = keyberon::layout::Layout<COLS, ROWS, N_LAYERS>;

pub type SharedLayout = Mutex<ThreadModeRawMutex, RefCell<Layout>>;

pub fn new_shared_layout() -> SharedLayout {
    Mutex::new(RefCell::new(keyberon::layout::Layout::new(&LAYERS)))
}

#[rustfmt::skip]
pub static LAYERS: Layers  = keyberon::layout::layout! {
    {
//   RX|  L0   |   L1   |   L2   |   L3   |   L4   |   L5   |   R0   |   R1   |   R2   |   R3   |   R4   |   R5   |
/*TX0*/[Grave   Kb1      Kb2      Kb3      Kb4      Kb5      Kb6      Kb7      Kb8      Kb9      Kb0      BSpace  ]
/*TX1*/[Tab     Q        W        E        R        T        Y        U        I        O        P        Bslash  ]
/*TX2*/[LCtrl   A        S        D        F        G        H        J        K        L        SColon   Quote   ]
/*TX3*/[LShift  Z        X        C        V        B        N        M        Comma    Dot      Slash    RShift  ]
/*TX4*/[No      No       No       LAlt     LGui     Space    Space    Enter    No       No       No       No      ],
        
            
    }
};
