use iced::theme::Palette;

// Light variant:
// // https://www.realtimecolors.com/?colors=493628-E4E0E1-8CC26F-e9c359-C14C3D&fonts=Inter-Inter
// pub const PALETTE: Palette = Palette {
//     background: iced::color!(0xe4e0e1),
//     text: iced::color!(0x493628),
//     primary: iced::color!(0xab886d),
//     success: iced::color!(0x8cc26f),
//     warning: iced::color!(0xd6b354),
//     danger: iced::color!(0xc14c3d),
// };

// https://www.realtimecolors.com/?colors=E4E0E1-2a2017-8CC26F-d6b354-C14C3D&fonts=Inter-Inter
pub const PALETTE: Palette = Palette {
    background: iced::color!(0x2a2017), // iced::color!(0xe4e0e1),
    text: iced::color!(0xe4e0e1),       // iced::color!(0x493628),
    primary: iced::color!(0xd6b354),    // iced::color!(0xab886d),
    success: iced::color!(0x8cc26f),
    warning: iced::color!(0xd6b354),
    danger: iced::color!(0xc14c3d),
};

pub const SPACING: f32 = 8.;
pub const PADDING: f32 = 8.;
pub const RADIUS: f32 = 8.;
