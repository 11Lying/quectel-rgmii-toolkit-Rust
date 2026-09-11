# 功能移植矩阵

本项目不以 capability 状态判定用户功能。没有 RM502Q-AE 实机时，保留既有入口，设备验证只决定后续 parser/transport 的最小修正。

| 功能 | 当前移植结论 |
|---|---|
| Dashboard、设备信息、SIM、信号、小区、CA、温度 | 保留现有 `At` 查询和 parser，经 USB serial transport 执行。 |
| Band/Frequency/Cell lock、扫描、APN/PDP、网络模式、IMEI、重启、AT&F | 保留；写操作要求参数校验与明确确认。 |
| SMS 收发、删除、ME/SM storage、转发 | 保留；USB transport 保持 prompt、URC 和 storage restore 语义。 |
| AT cache、兼容 AT API、Modem AT Console | 保留；不提供 Linux shell/PTY，但允许普通 modem AT。 |
| Traffic telemetry | 保留 `QGDNRCNT`/`QGDCNT` 查询及内存趋势。 |
| Ping、TTL、RM520N CPU/RAM、内部 Linux shell/PTTY | 已按明确范围移除。 |
| QMAP/QETH/usbnet/RGMII/PCIe gateway/data-plane 控制 | 已按明确范围移除；仅保留历史 parser/fixture 兼容。 |
| QMI、cdc-wdm、qmi_wwan、wwan0、netifd、路由/DNS/firewall/OpenClash | 由 ImmortalWrt 管理；SimpleAdmin 只可读取主机网络状态。 |
| USB serial discovery | 支持 by-id、`ttyUSB*`、`ttyACM*` 候选；不作为 capability gate。 |
| Windows installer/ADB | 历史资产，不属于 ImmortalWrt 运行包。 |