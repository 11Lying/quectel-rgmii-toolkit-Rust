# SimpleAdmin for RM502Q-AE on ImmortalWrt

SimpleAdmin 的 Quectel modem WebUI 移植版本，目标环境为 H5000M、ImmortalWrt、ARM64 和 RM502Q-AE。

它保留 baseline 中的 modem-management 业务：WebUI、`At` 队列和缓存、AT parser、HTTP API、SMS 和 Modem AT Console。移植只替换运行环境与 AT 连接方式：modem AT 通过 USB serial 访问，网络数据面交给 ImmortalWrt。

```text
SimpleAdmin WebUI / Modem AT Console
  -> existing modem-management logic (`At`)
  -> USB serial AT port
  -> RM502Q-AE

RM502Q-AE USB QMI
  -> /dev/cdc-wdmX + qmi_wwan
  -> wwan0
  -> ImmortalWrt netifd
  -> routing / DNS / firewall / OpenClash
```

SimpleAdmin 不负责 QMI 拨号、路由、防火墙、DNS、OpenClash 或 `wwan0` / netifd 生命周期。它只管理 modem AT control plane，并读取固定的主机网络状态。

## Upstream and baseline

- Upstream / baseline: [`tcpqueue/quectel-rgmii-toolkit-Rust`](https://github.com/tcpqueue/quectel-rgmii-toolkit-Rust)
- Baseline release: `v0.2.6`
- Baseline commit: `1f3dbf1a965c3c564f46cc1ea630b23c7fd2f686`
- License: MIT; upstream notices and third-party frontend licenses remain in the repository.

This is not a rewrite. The port keeps the baseline modem-management logic, AT commands, parser contracts, API behavior and WebUI where they apply to an external RM502Q-AE. Only the platform boundary and hardware transport were changed as required.

## Migration / porting

### Runtime environment

The baseline targeted an RM520N internal Linux/AP environment. This port runs `simpleadmin-httpd` on an external H5000M router running ImmortalWrt. The package targets the ImmortalWrt ARM64 musl toolchain.

### AT transport

`UsbAtTransport` replaces the fixed internal serial path. It accepts configured USB serial paths from `SIMPLEADMIN_AT_DEVICES`, `--at-devices`, or `/etc/simpleadmin/at_devices.conf`; if none are configured, discovery lists available `/dev/serial/by-id/*`, `/dev/ttyUSB*`, and `/dev/ttyACM*` candidates. No fixed tty number is assumed.

The USB transport uses raw 115200 baud serial I/O, a dedicated reader thread, bounded buffering, response termination for `OK`, `ERROR`, `+CME ERROR`, and `+CMS ERROR`, fragmented `+CMTI` notification handling, SMS prompt/PDU handling, storage restoration, and ESC cleanup after an interactive SMS failure. Grouped AT commands remain grouped except for required SMS setup before `AT+CMGS`.

Identify the actual interfaces on the target device before configuration:

```sh
lsusb
lsusb -t
dmesg | tail -n 200
ls -l /dev/serial/by-id /dev/ttyUSB* /dev/ttyACM* /dev/cdc-wdm* 2>/dev/null
ip link
```

Prefer a stable `/dev/serial/by-id/...` alias when available. USB VID/PID, interface numbering, tty mapping and default AT port for RM502Q-AE remain hardware-validation items.

### Network data plane

ImmortalWrt owns the QMI path: `qmi_wwan`, `/dev/cdc-wdmX`, `wwan0`, netifd, IPv4/IPv6 addressing, routing, DNS, firewall and OpenClash. SimpleAdmin does not invoke `uqmi`, `ifup`, `ifdown`, network reloads, or modem data-plane setup.

## Preserved modem-management features

Except for the removed legacy scope below, baseline modem-management behavior is retained:

- Dashboard, device information, SIM status, signal, serving cell, CA and temperature.
- `QGDNRCNT` / `QGDCNT` traffic counters, in-memory traffic trend and rate display.
- Band lock/reset, frequency selection, LTE and NR cell lock, cell scan, persistent cell lock and auto-unlock.
- SIM slot switching with `AT+QUIMSLOT`.
- Network mode, NR disable mode and APN/PDP configuration.
- SMS listing, send, delete, ME/SM storage handling and forwarding.
- AT cache, compatible manual-AT API and browser Modem AT Console.
- IMEI read/write, modem reboot/reset and `AT&F` reset, with explicit confirmation for writes.
- Existing parser, SMS, forwarding and cell-lock behavior unless a target response format later requires a minimal parser or transport correction.

RM502Q-AE has not yet been validated on hardware. That does not hide, remove or return a hardware-capability denial for these normal modem features.

## Removed legacy features

The following baseline features are deliberately not migrated:

- RM520N internal CPU and RAM monitoring, and TTL control. They are not replaced with H5000M host CPU/RAM monitoring.
- Ping telemetry: Ping target/configuration and persistence, RTT, jitter, packet loss, history, charts, summaries, `/api/get_ping`, and `/api/telemetry/target`.
- Internal Linux/AP control: shell, PTY, `/bin/sh`, arbitrary Linux command execution and internal system management.
- RM520N gateway/data-plane control: active QMAP, NAT, DHCP, DNS, DMZ, LAN IP, IP passthrough, RGMII, legacy PCIe data-plane topology, old AP/gateway behavior, `QCFG="usbnet"` and USB-composition control.

Historical QMAP parser or fixture support may remain for cache and test compatibility. It does not restore active QMAP data-plane control. The normal Modem AT Console remains available; it is not a router shell.

## AT architecture

```text
WebUI / compatible manual-AT API / Modem AT Console
  -> Axum HTTP and WebSocket API
  -> At queue, cache, parser and modem actions
  -> UsbAtTransport
  -> /dev/serial/by-id/*, /dev/ttyUSB*, or /dev/ttyACM*
  -> RM502Q-AE
```

There is no Adapter/profile layer or capability feature gate. Manual AT and Console writes still require authentication, validation, an explicit confirmation, and audit logging. Only explicitly retired gateway/data-plane AT command families are rejected.

## Build and install

### Build an ImmortalWrt package

Use the matching ImmortalWrt SDK and its ARM64 musl toolchain. Place this repository where the package Makefile expects it, then run from the SDK root:

```sh
make package/simpleadmin/compile V=s
```

The generated package is `bin/packages/*/base/simpleadmin_*.ipk`. See [docs/BUILD_IMMORTALWRT.md](docs/BUILD_IMMORTALWRT.md).

### Install on the router

1. Configure the QMI WAN interface in ImmortalWrt first; confirm `qmi_wwan`, `/dev/cdc-wdmX`, and `wwan0` work independently of SimpleAdmin.
2. Install `simpleadmin_*.ipk`.
3. Configure `/etc/config/simpleadmin`. Its defaults bind the WebUI to `192.168.1.1:8080`, use `/usr/share/simpleadmin/www`, and read authentication from `/etc/simpleadmin/auth`.
4. Create a password hash; do not put a plaintext password in the auth file:

   ```sh
   mkdir -p /etc/simpleadmin
   umask 077
   printf %s "choose-a-password" | /usr/bin/simpleadmin-httpd passwd
   chmod 600 /etc/simpleadmin/auth
   ```

5. Configure the AT port in `/etc/simpleadmin/at_devices.conf` if automatic discovery is not appropriate. Use one path per line, for example `/dev/serial/by-id/...` or `/dev/ttyUSB2`.
6. Enable and start the service:

   ```sh
   /etc/init.d/simpleadmin enable
   /etc/init.d/simpleadmin start
   ```

7. Open `http://192.168.1.1:8080/`, or the `listen_addr` and `port` configured in UCI.

The runtime binary is `/usr/bin/simpleadmin-httpd`; static files are `/usr/share/simpleadmin/www/`; configuration and persistent application state are under `/etc/simpleadmin/`. The procd service starts HTTP with `--no-tls=true` using the UCI listener values. See [docs/DEVICE_DEPLOY.md](docs/DEVICE_DEPLOY.md).

### Local development checks

```sh
cargo fmt --check
cargo check
cargo test
node tests/traffic-ui.cjs
```

The Rust test suite uses mock AT data. It does not replace device validation.

## Hardware validation

Software checks pass, but RM502Q-AE on H5000M has not yet been validated. On first hardware access, record the raw results described in [docs/DEVICE_FIRST_BOOT.md](docs/DEVICE_FIRST_BOOT.md) and [docs/RM502QAE_FIRST_AT.md](docs/RM502QAE_FIRST_AT.md).

Validate, at minimum:

1. USB enumeration, `/dev/serial/by-id`, `ttyUSB`/`ttyACM`, `cdc-wdm`, and QMI interface mapping.
2. `AT`, `ATI`, manufacturer/model/firmware, SIM state and SIM switching.
3. Signal, serving cell, CA, band queries, band/frequency/cell lock, scan, network mode and NR-disable behavior.
4. APN/PDP, SMS read/send/delete/forwarding, temperature, reboot/reset, IMSI/IMEI operations and `AT&F`.
5. QMI, `wwan0`, IPv4, IPv6 and netifd operation outside SimpleAdmin.
6. `QGDNRCNT` / `QGDCNT` traffic-counter format and resulting traffic telemetry.

Unverified hardware formats are not disabled features. They only determine whether a small parser or transport adjustment is required after raw responses are collected.

## Project structure

```text
src/                         Rust HTTP service and modem logic
development/simpleadmin/www/ WebUI assets packaged to /usr/share/simpleadmin/www
package/simpleadmin/         ImmortalWrt package Makefile, UCI defaults and procd service
tests/                       Rust integration modules, fixtures and browser/Node regressions
docs/                        Architecture, deployment, validation and migration records
scripts/                     Historical build/package helper scripts
installer/, windows-test/    Historical Windows baseline assets; not ImmortalWrt runtime dependencies
```

Important backend files:

- `src/at.rs`: serialized `At` queue, cache, page command groups, SMS transactions and USB reconnection boundary.
- `src/at_transport.rs`: raw USB serial transport, reader thread, URC and interactive SMS handling.
- `src/parser.rs`: modem response parsing, including serving-cell, CA, PDP and traffic-counter data.
- `src/actions.rs`, `src/cell_lock.rs`: validated modem setting actions, band/cell lock and persistence.
- `src/sms.rs`, `src/forwarding.rs`, `src/cleanup.rs`: SMS PDU handling, forwarding and deferred cleanup.
- `src/telemetry.rs`: signal and modem traffic in-memory history; Ping is intentionally absent.
- `src/server.rs`, `src/webui.rs`, `src/auth.rs`: Axum API, WebUI serving and authentication.
- `src/console.rs`, `src/console.html`: authenticated WebSocket Modem AT Console.
- `src/usb_discovery.rs`, `src/network_status.rs`: serial candidate discovery and fixed read-only host network status.

## Documentation

- [Migration record](docs/MIGRATION.md)
- [Architecture](docs/ARCHITECTURE.md)
- [ImmortalWrt build](docs/BUILD_IMMORTALWRT.md)
- [Device deployment](docs/DEVICE_DEPLOY.md)
- [First boot collection](docs/DEVICE_FIRST_BOOT.md)
- [First RM502Q-AE AT capture](docs/RM502QAE_FIRST_AT.md)
- [Porting audit](docs/PORTING_AUDIT.md)

## License and attribution

This repository remains MIT licensed under [LICENSE](LICENSE). The port retains the baseline project’s attribution and the frontend license notices in `development/simpleadmin/www/licenses/`. Quectel trademarks belong to Quectel Wireless Solutions; this is not an official Quectel tool.
