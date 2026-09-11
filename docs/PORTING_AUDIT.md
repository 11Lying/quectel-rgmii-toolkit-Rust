# ImmortalWrt RM502Q-AE 移植审计

基线：v0.2.6，`1f3dbf1a965c3c564f46cc1ea630b23c7fd2f686`。当前没有 RM502Q-AE 实机，但未验证不是 capability gate 或删减正常 modem 功能的理由。

## 已确认保留

- `At` 继续承担串行队列、缓存、parser、页面聚合、SMS、转发、锁频与 telemetry。
- `UsbAtTransport` 仅替换连接层，处理 tty、超时、URC、SMS prompt/PDU 与 ME/SM storage restore。
- Dashboard 保留 `QGDNRCNT`/`QGDCNT` 查询，维持原流量速率趋势。
- WebUI 保留 IMEI、重启、AT&F、普通 modem AT、兼容 AT API 和 Modem AT Console；写操作使用确认而非 capability 拒绝。
- USB discovery 只列出 serial 候选，不能裁决 API 或 UI 功能。

## 退休范围

只移除 Ping、TTL、RM520N 内部 CPU/RAM、内部 Linux shell/PTY/任意系统命令，以及 QMAP/QETH/usbnet/RGMII/PCIe gateway/data-plane 管理。历史 QMAP parser 和 fixture 可兼容保留，不代表活动控制。

## C. 已完成的移植

| 区域 | 路径 | 原问题与目标 |
|---|---|---|
| AT transport | `src/at.rs`, `src/at_transport.rs` | 已替换固定内部串口，USB serial 保持超时、URC 与 SMS 事务语义。 |
| USB 发现 | `src/usb_discovery.rs` | 列出 by-id、ttyUSB 和 ttyACM 候选，不固定 tty 编号，也不进行能力裁决。 |
| Modem 业务边界 | `At` 与直接 AT actions | 保持既有业务路径；不引入 Adapter/profile 层。 |
| AT policy | `src/at_policy.rs` | 手工 modem AT 保留；只拒绝明确退休的 gateway/data-plane 命令，写入仍需确认。 |
| 网络状态 | `src/network_status.rs` | 仅固定 `ip` 读取，不拨号或修改网络、路由、DNS、防火墙或 WAN 生命周期。 |
| App config | `src/main.rs`, `src/server.rs` | 使用 ImmortalWrt 路径与 USB serial 候选。 |
| API | `src/server.rs`, `src/console.rs` | 保留兼容 AT API、Console、IMEI、reboot、AT&F 和正常 modem actions。 |
| persistence platform | `src/persistence.rs` | 不 remount root；使用目标系统路径。 |
| 部署 | `package/simpleadmin/` | 使用 procd/UCI/OpenWrt package；Windows/systemd 资产不作为运行依赖。 |
| root credentials | `src/main.rs` | `root-password-init` 停用，由 ImmortalWrt 自己管理 root 凭据。 |

## D. 平台边界

| 功能 | 处理 |
|---|---|
| Linux shell/PTY | SimpleAdmin 不提供 router shell 或任意系统命令；Modem AT Console 保留且仅执行 AT。 |
| 原始 modem AT | 兼容 API 与 Console 保留；仅退休 gateway/data-plane AT 被拒绝，写入需要确认。 |
| USB mode/QCFG、QMAP/QETH/RGMII/PCIe | 不进行活动 runtime 控制；QMI/WAN 生命周期由 netifd 管理。 |
| IMEI、reboot、AT&F | 保留正常入口，不因未实机验证而隐藏或返回 capability 403。 |
| 网络拨号/生命周期 | 不调用 uqmi、ifup/ifdown 或 network reload，不管理 wwan0。 |
| rootfs remount | 不移植。 |
| systemd/Windows installer | 历史资产，不作为 ImmortalWrt 运行依赖。 |

## E. 仍待 RM502Q-AE 实机确认

- RM502Q-AE 的 USB VID/PID、serial interface 编号、AT tty 与 QMI cdc-wdm 映射。
- `ATI`、`AT+CGMI`、`AT+CGMM`、`AT+QGMR` 实际版本返回。
- `AT+QENG="servingcell"` 的字段顺序、LTE/NR/NSA/SA 具体格式。
- `AT+QCAINFO` 实际 CA 行格式及是否支持该命令。
- `AT+CSQ`、`AT+QRSRP`、`AT+QTEMP` 在目标固件上的字段/错误返回。
- SIM 状态命令、短信存储位置、PDU/CMGL/CMGD/CMGS 的目标行为。
- USB serial 默认波特率、URC 行为、断开重连特征。
- `qmi_wwan`、`/dev/cdc-wdmX` 与 `wwan0` 的实际枚举及 H5000M 固件支持。
- netifd QMI 配置字段和运营商 APN；由设备到货后按 ImmortalWrt 实机确认。

## 当前结论

项目保留成熟的 WebUI、API、parser、SMS、forwarding、telemetry、认证与测试。移植核心是以 USB serial transport 代替固定内部串口，同时保持 AT 业务路径；QMI/wwan0 继续由 ImmortalWrt netifd 管理。