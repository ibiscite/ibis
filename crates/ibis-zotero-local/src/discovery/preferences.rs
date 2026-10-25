//! Read preference data without evaluating code; unsupported syntax fails closed.

pub(super) fn data_directory(text: &str) -> Result<Option<String>, ()> {
    let mut parser = Parser(text.as_bytes());
    let (mut enabled, mut directory) = (false, None);
    while !parser.space()?.is_empty() {
        parser.eat(b"user_pref")?;
        parser.eat(b"(")?;
        let key = parser.string()?;
        parser.eat(b",")?;
        let value = parser.value()?;
        parser.eat(b")")?;
        parser.eat(b";")?;
        match (key.as_str(), value) {
            ("extensions.zotero.useDataDir", Value::Bool(value)) => enabled = value,
            ("extensions.zotero.dataDir", Value::String(value)) => directory = Some(value),
            ("extensions.zotero.useDataDir" | "extensions.zotero.dataDir", _) => return Err(()),
            _ => {}
        }
    }
    if !enabled {
        return Ok(None);
    }
    directory.filter(|value| !value.is_empty()).map(Some).ok_or(())
}

enum Value {
    String(String),
    Bool(bool),
    Integer,
}

struct Parser<'a>(&'a [u8]);

impl<'a> Parser<'a> {
    fn space(&mut self) -> Result<&'a [u8], ()> {
        loop {
            while self.0.first().is_some_and(u8::is_ascii_whitespace) {
                self.0 = &self.0[1..];
            }
            if self.0.starts_with(b"//") || self.0.starts_with(b"#") {
                let end = self
                    .0
                    .iter()
                    .position(|byte| matches!(byte, b'\r' | b'\n'))
                    .unwrap_or(self.0.len());
                self.0 = &self.0[end..];
            } else if self.0.starts_with(b"/*") {
                let end = self.0[2..].windows(2).position(|bytes| bytes == b"*/").ok_or(())?;
                self.0 = &self.0[end + 4..];
            } else {
                return Ok(self.0);
            }
        }
    }

    fn eat(&mut self, token: &[u8]) -> Result<(), ()> {
        self.0 = self.space()?.strip_prefix(token).ok_or(())?;
        Ok(())
    }

    fn value(&mut self) -> Result<Value, ()> {
        if matches!(self.space()?.first(), Some(b'"' | b'\'')) {
            return self.string().map(Value::String);
        }
        let end = self
            .0
            .iter()
            .position(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'+' | b'-'))
            .unwrap_or(self.0.len());
        let token = std::str::from_utf8(&self.0[..end]).map_err(|_| ())?;
        self.0 = &self.0[end..];
        match token {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => token.parse::<i32>().map(|_| Value::Integer).map_err(|_| ()),
        }
    }

    fn byte(&mut self) -> Result<u8, ()> {
        let (&byte, rest) = self.0.split_first().ok_or(())?;
        self.0 = rest;
        Ok(byte)
    }

    fn hex(&mut self, count: usize) -> Result<u16, ()> {
        let mut value = 0;
        for _ in 0..count {
            let digit = char::from(self.byte()?).to_digit(16).ok_or(())?;
            value = value * 16 + digit as u16;
        }
        Ok(value)
    }

    fn string(&mut self) -> Result<String, ()> {
        self.space()?;
        let quote = self.byte()?;
        if !matches!(quote, b'"' | b'\'') {
            return Err(());
        }
        let mut output = Vec::new();
        loop {
            let byte = self.byte()?;
            match byte {
                byte if byte == quote => break,
                b'\r' | b'\n' | 0 => return Err(()),
                b'\\' => match self.byte()? {
                    byte @ (b'\\' | b'"' | b'\'') => output.push(byte),
                    b'n' => output.push(b'\n'),
                    b'r' => output.push(b'\r'),
                    b'x' => output.push(self.hex(2)? as u8),
                    b'u' => {
                        let first = self.hex(4)?;
                        let scalar = if (0xd800..=0xdbff).contains(&first) {
                            // A UTF-16 surrogate pair must be adjacent within the string.
                            if self.byte()? != b'\\' || self.byte()? != b'u' {
                                return Err(());
                            }
                            let second = self.hex(4)?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err(());
                            }
                            0x10000 + (u32::from(first) - 0xd800) * 0x400 + u32::from(second) - 0xdc00
                        } else {
                            u32::from(first)
                        };
                        let character = char::from_u32(scalar).ok_or(())?;
                        output.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                    }
                    _ => return Err(()),
                },
                byte => output.push(byte),
            }
        }
        if output.contains(&0) {
            return Err(());
        }
        String::from_utf8(output).map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::data_directory;

    #[test]
    fn custom_directory_requires_the_switch_and_uses_the_last_value() {
        let prefs = r#"
            user_pref("extensions.zotero.dataDir", "/obsolete");
            user_pref("extensions.zotero.lastDataDir", "/ignored");
            user_pref("extensions.zotero.dataDir", "/synthetic/研究");
            user_pref("extensions.zotero.useDataDir", true);
        "#;
        assert_eq!(data_directory(prefs), Ok(Some("/synthetic/研究".into())));
        assert_eq!(data_directory(""), Ok(None));
        assert_eq!(
            data_directory(r#"user_pref("extensions.zotero.dataDir", "/ignored");"#),
            Ok(None)
        );
    }
}

#[cfg(test)]
mod hostile_tests {
    use super::data_directory;

    #[test]
    fn comments_and_unrelated_values_cannot_activate_settings() {
        let prefs = r#"
            /* user_pref("extensions.zotero.useDataDir", true);
               user_pref("extensions.zotero.dataDir", "/comment"); */
            // user_pref("extensions.zotero.useDataDir", true);
            # user_pref("extensions.zotero.useDataDir", true);
            user_pref("unrelated.integer", -123);
            user_pref("unrelated.string", "extensions.zotero.useDataDir");
            user_pref("extensions.zotero.dataDir", "/ignored");
        "#;
        assert_eq!(data_directory(prefs), Ok(None));
    }

    #[test]
    fn windows_escapes_unicode_and_comments_between_tokens_are_data() {
        let prefs = r#"
            user_pref /* comment */ ('extensions.zotero.useDataDir', true);
            user_pref("extensions.zotero.dataDir", "C:\\資料\\\"quoted\"\\\uD83D\uDE00\x21");
        "#;
        assert_eq!(data_directory(prefs), Ok(Some("C:\\資料\\\"quoted\"\\😀!".into())));
        assert_eq!(
            data_directory(&(prefs.to_owned() + "user_pref('extensions.zotero.useDataDir', false);")),
            Ok(None)
        );
    }

    #[test]
    fn invalid_syntax_types_and_nul_fail_closed() {
        for prefs in [
            "/* never closed",
            "run_arbitrary_code();",
            r#"user_pref("extensions.zotero.useDataDir", "true");"#,
            r#"user_pref("extensions.zotero.dataDir", false);"#,
            r#"user_pref("extensions.zotero.useDataDir", true);"#,
            r#"user_pref("extensions.zotero.useDataDir", true); user_pref("extensions.zotero.dataDir", "");"#,
            r#"user_pref("extensions.zotero.dataDir", "\uD800");"#,
            r#"user_pref("extensions.zotero.dataDir", "\uDC00");"#,
            r#"user_pref("extensions.zotero.dataDir", "\uD800\u0041");"#,
            r#"user_pref("extensions.zotero.dataDir", "\x00");"#,
            r#"user_pref("extensions.zotero.dataDir", "\u0000");"#,
            r#"user_pref("extensions.zotero.dataDir", "\xFF");"#,
            r#"user_pref("extensions.zotero.dataDir", "\unknown");"#,
            r#"user_pref("unrelated", 2147483648);"#,
            r#"user_pref("unrelated", true || false);"#,
        ] {
            assert_eq!(data_directory(prefs), Err(()));
        }
        let prefs =
            r#"user_pref("extensions.zotero.useDataDir", true);user_pref("extensions.zotero.dataDir", "/synthetic");"#;
        for end in 1..prefs.len() {
            assert_eq!(data_directory(&prefs[..end]), Err(()));
        }
        assert_eq!(data_directory(&(prefs.to_owned() + "unexpected")), Err(()));
    }
}
