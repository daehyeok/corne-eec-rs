use core::cell::RefCell;
use embassy_sync::blocking_mutex::{raw::ThreadModeRawMutex, Mutex};
use keyberon::{
    action::{
        k, l,
        Action::{self, HoldTap},
        HoldTapAction, HoldTapConfig,
    },
    key_code::KeyCode::*,
    layout,
};
pub const COLS: usize = 12;
pub const ROWS: usize = 5;
pub const N_LAYERS: usize = 2;

pub type Layers = layout::Layers<COLS, ROWS, N_LAYERS>;
#[allow(dead_code)]
pub type Layout = layout::Layout<COLS, ROWS, N_LAYERS>;

pub type SharedLayout = Mutex<ThreadModeRawMutex, RefCell<Layout>>;

pub fn new_shared_layout() -> SharedLayout {
    Mutex::new(RefCell::new(layout::Layout::new(&LAYERS)))
}

const FNSPC: Action = HoldTap(&HoldTapAction {
    timeout: 200,
    tap_hold_interval: 0,
    config: HoldTapConfig::HoldOnOtherKeyPress,
    hold: l(1),
    tap: k(Space),
});

#[rustfmt::skip]
pub static LAYERS: Layers  = layout::layout! {
    {
//     | 00(L0) | 01(L1) | 02(L2) | 03(L3) | 04(L4) | 05(L5) | 06(R0) | 07(R1) | 08(R2) | 09(R3) | 10(R4) | 11(R5) |
/*Row0*/[Grave   Kb1      Kb2      Kb3      Kb4      Kb5      Kb6      Kb7      Kb8      Kb9      Kb0      BSpace  ]
/*Row1*/[Tab     Q        W        E        R        T        Y        U        I        O        P        Bslash  ]
/*Row2*/[LCtrl   A        S        D        F        G        H        J        K        L        SColon   Quote   ]
/*Row3*/[LShift  Z        X        C        V        B        N        M        Comma    Dot      Slash    RShift  ]
/*Row4*/[No      No       No       LGui     LAlt     {FNSPC}  {FNSPC}  Enter    Down     Up       No       No     ]
    }{
//     | 00(L0) | 01(L1) | 02(L2) | 03(L3) | 04(L4) | 05(L5) | 06(R0) | 07(R1) | 08(R2) | 09(R3) | 10(R4) | 11(R5) |
/*Row0*/[No      No       No       No       No       No       No       No       No       Minus    Equal    No      ]
/*Row1*/[No      No       No       No       No       No       No       No       No       LBracket RBracket No      ]
/*Row2*/[No      No       No       No       No       No       No       Left     Down     Up       Right    No      ]
/*Row3*/[No      No       No       No       No       No       No       No       No       No       No       No      ]
/*Row4*/[No      No       No       No       No       No       No       No       No       No       No       No      ]
    }
};
