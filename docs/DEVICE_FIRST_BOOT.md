# 首次上电只读采集

设备到货后先采集，不执行改变模块配置或网络生命周期的命令：

```sh
uname -a
ubus call system board
cat /etc/openwrt_release
lsusb
lsusb -t
ls -l /dev/serial/by-id /dev/ttyUSB* /dev/ttyACM* /dev/cdc-wdm* 2>/dev/null
ip link
ip addr
ip route
ip -6 route
lsmod | grep -E 'qmi|wwan|usbserial'
dmesg | tail -n 200
test -x /usr/sbin/uqmi && uqmi --version
```

仅使用 `AT`、`ATI`、`AT+CGMI`、`AT+CGMM`、`AT+QGMR`、`AT+CSQ`、`AT+CPIN?`、`AT+QENG="servingcell"`、`AT+QCAINFO` 采集原始返回。PID、interface、tty 映射和字段仍为 `UNVERIFIED`，直到完成 RM502Q-AE 实机记录。
