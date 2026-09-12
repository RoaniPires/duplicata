use duplicata_core::hotkey::{
    format_hotkey, validate_hotkey, HotkeyCombo, HotkeyRejection, MOD_ALT, MOD_CONTROL, MOD_SHIFT,
    MOD_WIN,
};

#[test]
fn no_modifier_is_rejected() {
    let combo = HotkeyCombo {
        modifiers: 0,
        vkey: HotkeyCombo::DEFAULT.vkey,
    };
    assert_eq!(validate_hotkey(&combo), Err(HotkeyRejection::NoModifier));
}

#[test]
fn modifier_only_is_rejected() {
    let combo = HotkeyCombo {
        modifiers: MOD_CONTROL,
        vkey: 0,
    };
    assert_eq!(validate_hotkey(&combo), Err(HotkeyRejection::ModifierOnly));
}

#[test]
fn any_combination_with_win_is_rejected() {
    for base in [MOD_CONTROL, MOD_ALT, MOD_SHIFT, MOD_CONTROL | MOD_SHIFT] {
        let combo = HotkeyCombo {
            modifiers: base | MOD_WIN,
            vkey: HotkeyCombo::DEFAULT.vkey,
        };
        assert_eq!(
            validate_hotkey(&combo),
            Err(HotkeyRejection::ReservedSystemCombo),
            "modifiers={base:#x}|MOD_WIN deveria ser rejeitado"
        );
    }
}

#[test]
fn ctrl_alt_delete_is_rejected_with_a_distinct_reason() {
    let combo = HotkeyCombo::parse("ctrl+alt+delete").expect("sintaxe válida");
    assert_eq!(
        validate_hotkey(&combo),
        Err(HotkeyRejection::ReservedSystemCombo)
    );
}

#[test]
fn ctrl_shift_esc_is_rejected_with_a_distinct_reason() {
    let combo = HotkeyCombo::parse("ctrl+shift+esc").expect("sintaxe válida");
    assert_eq!(
        validate_hotkey(&combo),
        Err(HotkeyRejection::ReservedSystemCombo)
    );
}

#[test]
fn valid_combinations_are_accepted() {
    for s in ["ctrl+shift+v", "ctrl+alt+k", "shift+alt+f5", "ctrl+9"] {
        let combo =
            HotkeyCombo::parse(s).unwrap_or_else(|| panic!("{s} deveria ter sintaxe válida"));
        assert_eq!(validate_hotkey(&combo), Ok(()), "{s} deveria ser aceito");
    }
}

#[test]
fn round_trip_parse_of_format_is_identity() {
    for combo in [
        HotkeyCombo::DEFAULT,
        HotkeyCombo::parse("alt+f12").unwrap(),
        HotkeyCombo::parse("ctrl+alt+shift+9").unwrap(),
        HotkeyCombo::parse("win+esc").unwrap(),
        HotkeyCombo::parse("ctrl+alt+delete").unwrap(),
    ] {
        let formatted = combo.format();
        let reparsed = HotkeyCombo::parse(&formatted)
            .unwrap_or_else(|| panic!("format() produziu algo não reparseável: {formatted}"));
        assert_eq!(reparsed, combo, "round-trip falhou para {formatted}");
    }
}

#[test]
fn parse_rejects_empty_tokens_from_stray_separators() {
    for s in ["ctrl++v", "+ctrl+v", "ctrl+v+"] {
        assert_eq!(
            HotkeyCombo::parse(s),
            None,
            "{s} deveria ter sintaxe inválida"
        );
    }
}

#[test]
fn parse_rejects_more_than_one_non_modifier_key() {
    assert_eq!(
        HotkeyCombo::parse("ctrl+a+b"),
        None,
        "duas teclas não-modificadoras não formam uma combinação válida"
    );
}

#[test]
fn parse_accepts_hex_vkey_syntax() {
    let combo = HotkeyCombo::parse("ctrl+0x56").expect("sintaxe hex válida");
    assert_eq!(combo.vkey, HotkeyCombo::DEFAULT.vkey);
}

#[test]
fn parse_rejects_f_key_numbers_out_of_range_and_non_numeric_suffixes() {
    for s in ["ctrl+f0", "ctrl+f25", "ctrl+fx"] {
        assert_eq!(HotkeyCombo::parse(s), None, "{s} deveria ser rejeitado");
    }
}

#[test]
fn parse_rejects_an_unrecognized_multi_character_key_name() {
    assert_eq!(
        HotkeyCombo::parse("ctrl+banana"),
        None,
        "nome de tecla desconhecido (nem letra/dígito único, nem f-tecla, nem hex) deve falhar"
    );
}

#[test]
fn format_falls_back_to_hex_for_a_vkey_without_a_known_name() {
    let combo = HotkeyCombo {
        modifiers: MOD_CONTROL,
        vkey: 0x03,
    };
    assert_eq!(combo.format(), "ctrl+0x3");
}

#[test]
fn format_hotkey_uses_the_display_convention_ctrl_shift_alt_then_key() {
    assert_eq!(format_hotkey(&HotkeyCombo::DEFAULT), "Ctrl+Shift+V");
    assert_eq!(
        format_hotkey(&HotkeyCombo {
            modifiers: MOD_CONTROL,
            vkey: 0x50,
        }),
        "Ctrl+P"
    );
    assert_eq!(
        format_hotkey(&HotkeyCombo {
            modifiers: MOD_SHIFT,
            vkey: 0x0D,
        }),
        "Shift+Enter"
    );
    assert_eq!(
        format_hotkey(&HotkeyCombo {
            modifiers: MOD_ALT | MOD_SHIFT | MOD_CONTROL,
            vkey: 0x39,
        }),
        "Ctrl+Shift+Alt+9"
    );
}

#[test]
fn format_hotkey_and_serialization_format_are_deliberately_different() {
    let c = HotkeyCombo::DEFAULT;
    assert_eq!(format_hotkey(&c), "Ctrl+Shift+V");
    assert_eq!(c.format(), "ctrl+shift+v");
    assert_ne!(format_hotkey(&c), c.format());
}

#[test]
fn serialization_round_trip_is_untouched_by_the_new_display_fn() {
    for combo in [
        HotkeyCombo::DEFAULT,
        HotkeyCombo::parse("alt+f12").unwrap(),
        HotkeyCombo::parse("ctrl+alt+shift+9").unwrap(),
        HotkeyCombo::parse("ctrl+alt+delete").unwrap(),
    ] {
        assert_eq!(HotkeyCombo::parse(&combo.format()), Some(combo));
    }
    assert_eq!(
        HotkeyCombo::parse("ctrl+shift+v"),
        Some(HotkeyCombo::DEFAULT)
    );
}
