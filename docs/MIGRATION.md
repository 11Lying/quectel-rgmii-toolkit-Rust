# RM502Q-AE / ImmortalWrt migration record

## Baseline and target

- Baseline repository: `tcpqueue/quectel-rgmii-toolkit-Rust`
- Baseline release: `v0.2.6`
- Baseline commit: `1f3dbf1a965c3c564f46cc1ea630b23c7fd2f686`
- Target: Quectel RM502Q-AE connected over USB serial to H5000M running ImmortalWrt on ARM64.

This is a transport and platform port. The existing `At` business layer, parser, API, WebUI, SMS, forwarding, cell-lock and telemetry paths remain the basis of the current project.

## Architecture change

The baseline depended on an RM520N internal Linux/AP topology and a fixed internal serial path. The current runtime path is:

```text
WebUI / modem AT Console -> At -> UsbAtTransport -> USB serial AT port -> RM502Q-AE
RM502Q-AE QMI -> cdc-wdm / qmi_wwan / wwan0 -> ImmortalWrt netifd
```

`UsbAtTransport` is the connection-layer replacement. It opens configured or discovered `ttyUSB`, `ttyACM` or `serial/by-id` paths at raw 115200 baud. `At` remains responsible for serialized commands, caching, page aggregation, parser invocation, SMS and modem actions.

QMI, `cdc-wdm`, `qmi_wwan`, `wwan0`, netifd, routing, DNS, firewall and OpenClash remain ImmortalWrt responsibilities. SimpleAdmin does not start or alter those services.

## Transport work

`src/at_transport.rs` implements the USB serial boundary:

- dedicated reader thread with a bounded synchronous channel;
- final-response detection for `OK`, `ERROR`, `+CME ERROR` and `+CMS ERROR`;
- fragmented-line `+CMTI` recognition for SMS wakeups;
- prompt detection and Ctrl-Z PDU submission for `AT+CMGS`;
- SMS setup commands before a grouped `AT+CMGS` transaction;
- ESC cleanup and input drain when an interactive SMS request fails;
- ME/SM storage selection and restoration for SMS list/delete paths;
- normal grouped AT behavior for non-interactive modem commands.

`src/usb_discovery.rs` discovers serial candidates without assuming a fixed tty number. Its output is discovery metadata only; it does not gate APIs or WebUI functions.

## Parser and telemetry adjustments

- `CGCONTRDP` parsing accepts only concrete IPv4/IPv6 values before replacing current address data.
- `QGDNRCNT?` and `QGDCNT?` remain in `At::DASHBOARD`; the parser and telemetry sampler continue to produce download/upload rate history.
- The normal traffic-counter restoration is distinct from removed Ping telemetry.

## Preserved behavior

The port preserves baseline modem management, including dashboard/device/SIM status, signal/cell/CA/temperature, band and frequency control, LTE/NR cell lock and scan, persistent lock/auto-unlock, network mode, NR disable, APN/PDP, SMS and forwarding, IMEI, reboot, `AT&F`, AT cache, compatible manual AT API, and Modem AT Console.

The WebUI’s `AT&F` action was restored after baseline comparison: `src/actions.rs` continues to map `reset_at` to `AT&F`; `development/simpleadmin/www/index.html` and `js/pages/settings.js` expose it with explicit confirmation.

No Adapter/profile architecture or capability state was retained. Absence of target hardware does not hide normal modem functions or produce a capability denial. Normal writes continue to require authentication, POST, parameter validation and explicit confirmation.

## Removed scope

The following is intentionally not ported:

- RM520N internal CPU/RAM metrics and TTL, with no replacement H5000M resource page.
- Ping target/settings, Ping RTT/jitter/loss/history/chart/summary, and Ping APIs.
- Internal Linux shell, PTY, `/bin/sh`, arbitrary system command execution and AP management.
- Active QMAP/QETH gateway/data-plane operations: NAT, DHCP, DNS, DMZ, LAN IP, IP passthrough, MPDN, RGMII, legacy PCIe topology, `QCFG="usbnet"` and USB composition control.

Historical QMAP parser/fixtures can remain for compatibility but do not execute active gateway/data-plane configuration. Modem AT Console remains modem-AT-only.

## Deployment artifacts

`package/simpleadmin/` adds the ImmortalWrt package definition, UCI defaults and procd service. The installed layout is:

```text
/usr/bin/simpleadmin-httpd
/usr/share/simpleadmin/www/
/etc/config/simpleadmin
/etc/init.d/simpleadmin
/etc/simpleadmin/auth
/etc/simpleadmin/at_devices.conf   # optional serial-port list
```

Authentication is initialized by piping a chosen password to `simpleadmin-httpd passwd`; the auth file contains a hash and must remain mode `0600`. ImmortalWrt root credentials are managed by ImmortalWrt; `root-password-init` is intentionally disabled.

## Validation completed before hardware

The current tree passed:

```text
cargo fmt --check
cargo check
cargo test              # 69 passed, 0 failed
git diff --check
node tests/traffic-ui.cjs
```

The tests cover AT queue/cache behavior, parser fixtures, SMS handling, forwarding, cell locks, authentication, HTTP mutations, WebUI port handling, USB discovery behavior and traffic UI cases. They use mock data and PTY transport tests; they do not prove RM502Q-AE firmware-specific response formats.

## Hardware work remaining

Before changing parser or transport behavior, capture raw USB enumeration and the read-only AT set documented in `RM502QAE_FIRST_AT.md`. Confirm VID/PID, interface-to-tty mapping, QMI mapping, `ATI` / `QGMR`, serving-cell and CA formats, SIM/SMS storage behavior, traffic counters, USB reconnect behavior and netifd QMI configuration on the actual H5000M firmware.
