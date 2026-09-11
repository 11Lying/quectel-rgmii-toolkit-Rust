use anyhow::bail;
use std::time::Duration;

pub const PENDING: &str = "AT command is running in background; cached data is not ready yet.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandClass {
    ReadOnly,
    SafeWrite,
    SensitiveWrite,
    Dangerous,
}

const READ_ONLY: &[&str] = &[
    "AT",
    "ATI",
    "AT+CGMI",
    "AT+CGMM",
    "AT+CGSN",
    "AT+GMR",
    "AT+QGMR",
    "AT+CIMI",
    "AT+ICCID",
    "AT+CNUM",
    "AT+QSIMSTAT?",
    "AT+CPIN?",
    "AT+QUIMSLOT?",
    "AT+CSQ",
    "AT+QTEMP",
    "AT+QSPN",
    "AT+QCAINFO",
    "AT+QRSRP",
    "AT+QENG=\"SERVINGCELL\"",
    "AT+CGDCONT?",
    "AT+CGCONTRDP=1",
    "AT+CMGF=0",
    "AT+CNMI=2,1,0,0,0",
    "AT+CMGL=4",
    "AT+CPMS?",
    "AT+CPMS=?",
    "AT+QGDNRCNT?",
    "AT+QGDCNT?",
    "AT+QNWPREFCFG=\"MODE_PREF\"",
    "AT+QNWPREFCFG=\"NR5G_DISABLE_MODE\"",
    "AT+QNWPREFCFG=\"LTE_BAND\"",
    "AT+QNWPREFCFG=\"NSA_NR5G_BAND\"",
    "AT+QNWPREFCFG=\"NR5G_BAND\"",
    "AT+QNWLOCK=\"COMMON/4G\"",
    "AT+QNWLOCK=\"COMMON/5G\"",
];

fn normalized(part: &str) -> String {
    part.trim().replace(' ', "").to_ascii_uppercase()
}

pub fn classify_part(part: &str) -> Option<CommandClass> {
    let command = normalized(part);
    if READ_ONLY.iter().any(|allowed| command == *allowed) {
        return Some(CommandClass::ReadOnly);
    }
    if command == "AT&F"
        || command.starts_with("AT+CFUN=")
        || command.starts_with("AT+EGMR=")
        || command.starts_with("AT+QCFG=\"USBNET\",")
        || command.starts_with("AT+QETH")
        || command.starts_with("AT+QMAPWAC=")
        || command.contains("+QMAP=\"MPDN_RULE\"")
        || command.contains("+QMAP=\"DHCPV")
        || command.starts_with("AT+QMAP=\"DMZ\"")
        || command.starts_with("AT+QMAP=\"LANIP\",")
        || command.contains("+RGMII")
        || command.contains("+PCIE")
    {
        return Some(CommandClass::Dangerous);
    }
    if command.starts_with("AT+CMGD")
        || command.starts_with("AT+CMGS")
        || command.starts_with("AT+QUIMSLOT=")
        || command.starts_with("AT+QNWLOCK=\"COMMON/4G\",")
        || command.starts_with("AT+QNWLOCK=\"COMMON/5G\",")
        || command.starts_with("AT+QNWPREFCFG=\"LTE_BAND\",")
        || command.starts_with("AT+QNWPREFCFG=\"NSA_NR5G_BAND\",")
        || command.starts_with("AT+QNWPREFCFG=\"NR5G_BAND\",")
        || command.starts_with("AT+QNWPREFCFG=\"MODE_PREF\",")
        || command.starts_with("AT+QNWPREFCFG=\"NR5G_DISABLE_MODE\",")
    {
        return Some(CommandClass::SensitiveWrite);
    }
    if command.starts_with("AT+QSCAN=") || command.starts_with("AT+CGDCONT=") {
        return Some(CommandClass::SafeWrite);
    }
    None
}

pub fn classify(command: &str) -> Option<CommandClass> {
    let mut result = None;
    for part in crate::at::split(command) {
        let class = classify_part(&part)?;
        result = Some(match (result, class) {
            (Some(CommandClass::Dangerous), _) | (_, CommandClass::Dangerous) => {
                CommandClass::Dangerous
            }
            (Some(CommandClass::SensitiveWrite), _) | (_, CommandClass::SensitiveWrite) => {
                CommandClass::SensitiveWrite
            }
            (Some(CommandClass::SafeWrite), _) | (_, CommandClass::SafeWrite) => {
                CommandClass::SafeWrite
            }
            (Some(CommandClass::ReadOnly), CommandClass::ReadOnly)
            | (None, CommandClass::ReadOnly) => CommandClass::ReadOnly,
        });
    }
    result
}

pub fn allowed(command: &str) -> bool {
    crate::at::split(command)
        .iter()
        .all(|part| classify_part(part) == Some(CommandClass::ReadOnly))
}

pub fn removed_gateway_command(command: &str) -> bool {
    crate::at::split(command).iter().any(|part| {
        let command = normalized(part);
        command.starts_with("AT+QCFG=\"USBNET\",")
            || command.starts_with("AT+QETH")
            || command.starts_with("AT+QMAPWAC=")
            || command.contains("+QMAP=\"MPDN_RULE\"")
            || command.contains("+QMAP=\"DHCPV")
            || command.starts_with("AT+QMAP=\"DMZ\"")
            || command.starts_with("AT+QMAP=\"LANIP\",")
            || command.contains("+RGMII")
            || command.contains("+PCIE")
    })
}

pub fn validate(command: &str) -> anyhow::Result<()> {
    if removed_gateway_command(command) {
        bail!("retired modem gateway/data-plane AT command")
    }
    Ok(())
}

pub fn action(command: &str) -> bool {
    let upper = command.to_ascii_uppercase();
    upper == "AT&F"
        || upper.contains(";AT&F")
        || [
            "+CFUN=",
            "+EGMR=",
            "+CMGD",
            "+CMGS",
            "+QSCAN=",
            "+CGDCONT=",
            "+QUIMSLOT=",
            "+QNWPREFCFG=",
            "+QNWLOCK=",
        ]
        .iter()
        .any(|pattern| upper.contains(pattern))
}

pub fn timeout(command: &str) -> Duration {
    let parts = crate::at::split(command);
    Duration::from_millis(
        parts
            .iter()
            .map(|part| {
                if normalized(part).contains("QSCAN") {
                    120000
                } else {
                    1000
                }
            })
            .sum::<u64>()
            .max(1000),
    )
}

pub fn max_age(command: &str) -> Duration {
    let upper = command.trim().to_ascii_uppercase();
    let secs = if action(command) {
        0
    } else if upper == "AT+CGMM" || upper == "AT+CGMI;+CGSN;+QGMR;+CIMI;+ICCID;+CNUM" {
        600
    } else if sms(command) {
        5
    } else if upper.contains("QSIMSTAT")
        || upper.contains("CPIN")
        || upper.contains("QCAINFO")
        || upper.contains("QENG")
        || upper.contains("QRSRP")
        || upper.contains("CSQ")
        || upper.contains("QTEMP")
    {
        3
    } else if upper.contains("CIMI")
        || upper.contains("ICCID")
        || upper.contains("CNUM")
        || upper.contains("CGMI")
        || upper.contains("CGSN")
        || upper.contains("QGMR")
    {
        600
    } else {
        30
    };
    Duration::from_secs(secs)
}

pub fn sms(command: &str) -> bool {
    let upper = command.to_ascii_uppercase();
    upper.contains("+CMGL=4") || upper.contains("+CMGL=\"ALL\"")
}

pub fn immediate(command: &str) -> bool {
    command.trim().eq_ignore_ascii_case("AT+CGMM")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dangerous_commands_are_not_allowed() {
        assert!(!allowed("AT+QCFG=\"usbnet\",1"));
        assert!(!allowed("AT+CFUN=1,1"));
        assert!(!allowed("AT+CSQ;+CFUN=1"));
        assert!(allowed("AT+CSQ;+QTEMP"));
        assert_eq!(
            classify("AT+QNWLOCK=\"common/4g\",0"),
            Some(CommandClass::SensitiveWrite)
        );
        assert!(validate("AT+QNWLOCK=\"common/4g\",0").is_ok());
        assert!(validate("AT+CFUN=1,1").is_ok());
        assert!(validate("AT+QCFG=\"usbnet\",1").is_err());
    }
}
