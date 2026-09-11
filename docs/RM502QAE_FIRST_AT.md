# RM502Q-AE 首次 AT 采集

硬件状态：`UNVERIFIED`。以下命令只读，逐条执行并保存完整原始返回：

```text
AT
ATI
AT+CGMI
AT+CGMM
AT+QGMR
AT+CGSN
AT+CSQ
AT+QRSRP
AT+QTEMP
AT+CPIN?
AT+QSIMSTAT?
AT+QUIMSLOT?
AT+QENG="servingcell"
AT+QCAINFO
AT+CGDCONT?
AT+CGCONTRDP=1
```

记录项目：USB VID/PID、USB interface、`/dev/serial/by-id`、所有 tty 与接口对应关系、`/dev/cdc-wdmX`、`wwan0`、QENG 字段顺序、QCAINFO 行格式、SIM/SMS 存储行为。不要执行 `AT+CFUN`、`AT+QCFG` 写入、`AT+QMAP` 写入、`AT+QNWPREFCFG` 写入、`AT+QNWLOCK` 写入、`AT+EGMR` 或任意复合写命令。
