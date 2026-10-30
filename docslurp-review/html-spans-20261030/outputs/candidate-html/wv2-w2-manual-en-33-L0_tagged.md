KESTREL AUTOMATION

# KX-400 Industrial Edge Gateway

### Installation and Operation Manual · Models KX-400, KX-400-LW, KX-400-EX · Firmware 3.2

KX-400 gateways collect data from presses, CNC machines and energy meters on the shop floor.

Revision history

Rev. Date Changes

<table>
<tbody>
<tr><td>A</td><td>2025-11-03</td><td>First release</td></tr>
<tr><td>B</td><td>2026-02-17</td><td>Added KX-400-LW cellular variant; corrected DI wiring diagram</td></tr>
</tbody>
</table>

C 2026-07-08 Firmware 3.2: OPC UA server, container runtime, new web UI screens

© 2026 Kestrel Automation Inc., 4180 Commerce Parkway, Dayton, OH 45414, USA. All rights reserved. Kestrel and EdgeView are trademarks of Kestrel Automation Inc. Specifications are subject to change without notice.## Contents

- 1. Safety information

- 2. Product overview

- 3. Technical specifications

- 4. Installation a. Mounting b. Wiring c. First power-up

- 5. Network configuration

- 6. Web interface

- 7. Data pipelines and integrations

- 8. Maintenance a. Routine checks b. Firmware update

- 9. Troubleshooting

- 10. Support, warranty and regulatory information

- 11. Appendix A: Modbus register map

## About this manual

This manual is intended for control-system integrators, electricians and IT/OT administrators who install, configure and maintain the KX-400 gateway. It assumes familiarity with industrial networks, basic Linux administration and the safety rules for work in electrical cabinets. Read the complete manual before installing the device and keep it available for later reference. The latest version, including localized editions in Spanish, French and Japanese (KX-400 取扱説明書), is available at

support.kestrel-automation.example/kx400.

Menu paths in the web interface are shown as Settings › Network › Ethernet. Values in angle brackets, for example <serial-number>, must be replaced by your own values.

Symbols used in this manual

### WARNING

Indicates a hazardous situation which, if not avoided, could result in death or serious injury.

### CAUTION

Indicates a situation which, if not avoided, could result in damage to the device or loss of data.

Note

Provides additional information or a recommended practice.# 1 Safety information

The KX-400 is an open-type device intended for installation in an enclosure that provides protection against electric shock and fire spread. Installation and electrical connection must only be carried out by qualified personnel in accordance with local regulations, such as NFPA 70 (National Electrical Code) in the United States or IEC 60364 elsewhere.

### WARNING — Electric shock

Disconnect all power sources before wiring the terminals. The supply terminals may carry hazardous voltage from adjacent equipment even when the gateway itself is switched off.

### WARNING — Explosion hazard

KX-400-EX units installed in Class I, Division 2 or ATEX Zone 2 hazardous locations must not be connected or disconnected while the circuit is live unless the area is known to be non-hazardous. Do not use the USB service port in hazardous locations.

## 1.1 Intended use

The gateway collects data from machines, sensors and controllers, processes it locally and forwards it to supervisory systems or cloud services. It is not a safety-rated controller. Never use the digital outputs for functions that protect people, such as emergency stop or light-curtain circuits.

Operate the device only within the ambient conditions listed in Table 3. Use a Class 2 or SELV/PELV power supply rated for at least 15 W. Keep at least 25 mm (1 in) of free space above and below the housing for convection cooling. Change the default password during commissioning; the device refuses remote access until you do. Dispose of the device according to local electronic-waste rules (see section 10.3).

### CAUTION

Electrostatic discharge can damage the device. Touch the grounded DIN rail before handling the terminal blocks or the microSD card.

## 1.2 Cybersecurity

Place the gateway in a dedicated OT network zone behind a firewall as described in IEC 62443-3-3. Disable unused services, keep the firmware up to date and restrict access to the web interface to engineering workstations. Report suspected vulnerabilities to psirt@kestrel-automation.example.# 2 Product overview

The KX-400 combines protocol conversion, local data processing and secure connectivity in a compact DIN-rail housing. Data from Modbus devices, OPC UA servers and the on-board I/O is collected by pipelines, optionally transformed by scripts or containerized applications and published to MQTT brokers, OPC UA clients or databases.

## 2.1 Package contents

Table 1. Package contents

<table>
<tbody>
<tr><td>Item</td><td>Description</td><td>Part number</td><td>Qty</td></tr>
<tr><td>1</td><td>KX-400 gateway</td><td>KX-400-00</td><td>1</td></tr>
<tr><td>2</td><td>Pluggable terminal block, 3-pin power (端子台 TB-2)</td><td>KX-TB2-P</td><td>2</td></tr>
<tr><td>3</td><td>Pluggable terminal block, 10-pin I/O</td><td>KX-TB10-IO</td><td>2</td></tr>
<tr><td>4</td><td>LTE antenna, SMA, 3 dBi (KX-400-LW only)</td><td>KX-ANT-L3</td><td>2</td></tr>
<tr><td>5</td><td>Wi-Fi/BLE antenna, RP-SMA, 2 dBi</td><td>KX-ANT-W2</td><td>1</td></tr>
<tr><td>6</td><td>Quick start guide (EN/ES/FR/日本語)</td><td>KX400-QSG</td><td>1</td></tr>
<tr><td>7</td><td>Grounding screw M4 × 8 with lock washer</td><td>HW-M4-8</td><td>1</td></tr>
</tbody>
</table>

Check the contents immediately after receipt. If items are missing or damaged, contact your distributor within seven days. Keep the packaging for returns; the device must be shipped in its original foam insert.

## 2.2 Front panel and LEDs

PWR — green when PWR1 or PWR2 is present; amber when only one of two configured supplies is present SYS — green: running; blinking green: booting; amber: warning; red: fault ETH1/ETH2 — link (green) and activity (flashing)

BUS — flashes with Modbus RTU traffic; red on communication errors LTE (-LW only) — signal strength in three steps RESET button — 3 s: reboot; 15 s: recovery mode; 30 s: factory reset The serial number, MAC addresses and the default device password are printed on the label on the right side of the housing. Note them before mounting, because the label is hidden once neighbouring devices are installed.# 3 Technical specifications

Table 3. Technical specifications (KX-400 and KX-400-LW)

<table>
<tbody>
<tr><td>Parameter</td><td>Specification</td></tr>
<tr><td>Processor</td><td>NXP i.MX 8M Plus, 4 × Cortex-A53 @ 1.6 GHz, NPU 2.3 TOPS</td></tr>
<tr><td>Memory</td><td>4 GB LPDDR4, 32 GB eMMC (pSLC mode 10 GB)</td></tr>
<tr><td>Storage expansion</td><td>1 × microSD (SDXC, up to 512 GB), industrial grade recommended</td></tr>
<tr><td>Ethernet</td><td>2 × 10/100/1000BASE-T (RJ45), independent MACs; ETH1 supports PoE PD(802.3at)</td></tr>
<tr><td>Serial</td><td>2 × RS-485 (isolated, 1.5 kV), 1 × RS-232 / RS-485 selectable</td></tr>
<tr><td>Fieldbus</td><td>Modbus RTU/TCP, OPC UA client and server, MQTT 3.1.1/5.0</td></tr>
<tr><td>Digital inputs</td><td>8 × 24 V DC, sink/source, 2 kHz counter on DI1–DI2</td></tr>
<tr><td>Digital outputs</td><td>4 × solid-state relay, 0.5 A @ 30 V DC</td></tr>
<tr><td>Analog inputs</td><td>4 × 0–10 V / 4–20 mA, 16-bit, ±0.1 % FS</td></tr>
<tr><td>Wireless (KX-400-LW)</td><td>LTE Cat-4 (bands 2/4/5/12/13/66/71), Wi-Fi 5, BLE 5.0</td></tr>
<tr><td>USB</td><td>1 × USB 3.0 host, 1 × USB-C service port</td></tr>
<tr><td>Real-time clock</td><td>Supercapacitor backup, 7 days; NTP / PTP (IEEE 1588) sync</td></tr>
<tr><td>Supply voltage</td><td>12–48 V DC (redundant inputs PWR1/PWR2), reverse-polarity protected</td></tr>
<tr><td>Power consumption</td><td>6.8 W typical, 11.5 W maximum (without PoE output)</td></tr>
</tbody>
</table>

¹ Values at 25 °C ambient unless otherwise stated. ² LTE bands differ for the EU variant KX-400-LW-E (1/3/7/8/20/28).Table 3. Technical specifications (continued)

<table>
<tbody>
<tr><td>Parameter</td><td>Specification</td></tr>
<tr><td>Operating temperature</td><td>−40 °C to +70 °C (−40 °F to +158 °F)</td></tr>
<tr><td>Storage temperature</td><td>−40 °C to +85 °C</td></tr>
<tr><td>Relative humidity</td><td>5–95 %, non-condensing</td></tr>
<tr><td>Shock / vibration</td><td>IEC 60068-2-27 (50 g) / IEC 60068-2-6 (5 g, 10–500 Hz)</td></tr>
<tr><td>Ingress protection</td><td>IP40 (housing), IP20 (terminals)</td></tr>
<tr><td>Dimensions (W × H × D)</td><td>54 × 142 × 118 mm (2.13 × 5.59 × 4.65 in)</td></tr>
<tr><td>Weight</td><td>620 g (1.37 lb)</td></tr>
<tr><td>Mounting</td><td>35 mm DIN rail (EN 60715) or wall bracket KX-WB1</td></tr>
<tr><td>MTBF</td><td>412,000 h (Telcordia SR-332, 40 °C)</td></tr>
<tr><td>Certifications</td><td>UL 61010-2-201, CE, FCC Part 15 B, IECEx/ATEX Zone 2 (option -EX)</td></tr>
</tbody>
</table>

## 3.1 Dimensions

The housing is 54 mm wide, so up to three gateways fit in the space of a standard 6-module circuitbreaker section. The terminal blocks protrude 14 mm from the front; allow an additional 40 mm for cable bending radius.

## 3.2 Derating

Above 55 °C ambient the maximum load on the digital outputs is reduced linearly to 0.25 A per channel at 70 °C. When the PoE input is used together with the cellular modem, the maximum ambient temperature is limited to 60 °C.

## 3.3 Accessories

Table 7. Accessories and licenses

<table>
<tbody>
<tr><td>Part number</td><td>Description</td></tr>
<tr><td>KX-WB1</td><td>Wall-mounting bracket, stainless steel</td></tr>
<tr><td>KX-PS24-2</td><td>DIN-rail power supply 24 V DC / 2.5 A, Class 2</td></tr>
<tr><td>KX-ANT-L3-5M</td><td>LTE antenna with 5 m cable, IP67, magnetic base</td></tr>
<tr><td>KX-SD64-I</td><td>Industrial microSD card 64 GB, pSLC, −40 to +85 °C</td></tr>
<tr><td>KX-TB-KIT</td><td>Spare terminal block kit (power and I/O)</td></tr>
<tr><td>KX-LIC-OPCUA</td><td>License: OPC UA server, 5,000 tags</td></tr>
<tr><td>KX-LIC-CONT</td><td>License: container runtime (2 containers)</td></tr>
</tbody>
</table>

Licenses are bound to the serial number of the gateway and are installed under System › Licenses. They remain valid after firmware updates and factory resets.# 4 Installation

## 4.1 Mounting on a DIN rail

Mount the gateway vertically with the terminals facing down so that heat can escape through the ventilation slots on the top. Horizontal mounting is permitted but reduces the maximum ambient temperature by 10 °C. You need a 35 mm top-hat rail, a slotted screwdriver and, for the wall bracket option, two M4 screws.

- 1. Prepare the rail a. Make sure the rail is connected to protective earth. b. Leave 25 mm free above and below.

- 2. Mount the gateway a. Hook the upper edge of the rear clip onto the rail. b. Press the lower edge until the clip snaps in. c. Check that the device sits firmly and cannot slide sideways.

- 3. Ground the housing a. Fit the M4 grounding screw with lock washer. b. Connect it to the ground bar with ≥ 1.5 mm² wire.

To remove the gateway, insert a screwdriver into the slot

of the rail clip, lever it downwards and tilt the device away from the rail.

Note

Figure 1. Recommended tools: 3 mm slotted screwdriver, wire stripper, ferrule crimper and torque screwdriver.

Do not mount the gateway directly above variable-frequency drives or other heat sources. Ambient temperature is measured 25 mm below the housing.## 4.2 Wiring

All terminals are pluggable spring-cage connectors. Use ferrules for stranded wire and strip 10 mm of insulation. Tighten the grounding screw to 1.2 N·m (10.6 lbf·in).

Table 2. Terminal assignment

<table>
<tbody>
<tr><td>Terminal</td><td>Signal</td><td>Description</td><td>Wire size</td></tr>
<tr><td>PWR1 +/−</td><td>12–48 V DC</td><td>Primary supply</td><td>0.5–2.5 mm² (AWG 20–14)</td></tr>
<tr><td>PWR2 +/−</td><td>12–48 V DC</td><td>Redundant supply (optional)</td><td>0.5–2.5 mm² (AWG 20–14)</td></tr>
<tr><td>FG</td><td>Functional ground</td><td>Connect to cabinet ground bar</td><td>≥ 1.5 mm² (AWG 16)</td></tr>
<tr><td>DI1–DI8, COM</td><td>24 V DC inputs</td><td>Common selectable sink/source</td><td>0.2–1.5 mm² (AWG 24–16)</td></tr>
<tr><td>DO1–DO4</td><td>SSR outputs</td><td>0.5 A max. per channel</td><td>0.2–1.5 mm² (AWG 24–16)</td></tr>
<tr><td>AI1–AI4, AGND</td><td>0–10 V / 4–20 mA</td><td>Mode set in web UI or CLI</td><td>Shielded twisted pair</td></tr>
<tr><td>A1/B1, A2/B2</td><td>RS-485</td><td>120 Ω termination switchable</td><td>Shielded twisted pair</td></tr>
</tbody>
</table>

*CAUTION*

Connecting more than 30 V DC to a digital input destroys the input circuit. Inputs are not protected against 120/230 V AC.

## 4.3 First power-up

- 1. Before switching on a. Check polarity at PWR1 with a multimeter. b. Insert the microSD card now if you use one.

- 2. Switch on the supply a. PWR turns green; SYS blinks green for about 40 s. b. SYS turns steady green when the system is ready.

- 3. Connect a laptop a. Use the USB-C service port or ETH2 (default 192.168.250.1/24). b. Open https://192.168.250.1 and accept the self-signed certificate.

- 4. Log in as admin with the password from the label and set a new password (at least 12 characters). The setup assistant then guides you through the time zone, network and NTP settings. You can skip the assistant and configure the device from the command line as described in the next section.# 5 Network configuration

ETH1 is configured for DHCP and ETH2 has the static address 192.168.250.1 by default. The ports are separate interfaces; routing between them is disabled, so the gateway can connect a machine network to a plant network without bridging the two.

Listing 1. Setting a static address on ETH1 from the console

ssh admin@192.168.250.1 kx-cli net show kx-cli net set eth1 --mode static --address 10.20.3.10/24 \

--gateway 10.20.3.1 --dns 10.20.0.53,10.20.0.54 kx-cli net apply --confirm-timeout 120

net apply activates the new settings and rolls them back automatically after 120 s unless you confirm them with kx-cli net confirm from the new address. This prevents a lock-out caused by a typing error.

For repeatable commissioning, the complete network configuration can also be supplied as a YAML file through the web interface (System › Configuration › Import) or on a USB stick inserted at boot:

Listing 2. Network configuration file

# /etc/kestrel/network.yaml interfaces:

eth1: mode: static address: 10.20.3.10/24 gateway: 10.20.3.1 dns: [10.20.0.53, 10.20.0.54] eth2:

mode: static address: 192.168.250.1/24 cellular: # KX-400-LW only apn: iot.carrier.example failover: true ntp:

servers: [10.20.0.10, time.nist.example] firewall:

default: deny allow:

- {port: 443, from: 10.20.5.0/24} # engineering VLAN

- {port: 502, from: 10.20.3.0/24} # Modbus TCP

### Note

The built-in firewall denies all incoming connections that are not listed. If you enable the OPC UA server, add port 4840 to the allow list.# 6 Web interface

The EdgeView web interface runs on the gateway and requires no installation. Supported browsers are current versions of Chrome, Edge and Firefox. The dashboard shows the system status, the most important I/O values and the state of all data pipelines at a glance (Figure 2).

Figure 2. EdgeView dashboard of a gateway in operation (firmware 3.2)

Dashboard — system health, message rates and active alarms I/O — live values, input filters and output test Fieldbus — Modbus and OPC UA device configuration Pipelines — data flows from sources to destinations Containers — install and monitor Docker-compatible applications System — users, certificates, firmware, backup# 7 Data pipelines and integrations

A pipeline connects one source to one or more destinations. Each pipeline has a sampling interval, optional deadband filters and a payload template. Pipelines are stored as JSON and can be exported, version-controlled and imported on other gateways.

Listing 3. Pipeline definition (Pipelines › Import)

{

"name": "press-line-3", "source": {"type": "modbus-tcp", "host": "10.20.3.41", "unit": 1,

"registers": [{"addr": 30001, "count": 4, "type": "float32"}]}, "interval_ms": 1000, "deadband": {"absolute": 0.05}, "destinations": [ {"type": "mqtt", "broker": "mqtts://10.20.0.80:8883", "topic": "plant/line3/press/{register}", "qos": 1, "retain": false}

]

}

Values can also be read programmatically through the REST API. Every request needs an API token, which you create under System › Users › API tokens. The following Python example reads the analog inputs once per second:

Listing 4. Reading analog inputs through the REST API

import time import requests

GATEWAY = "https://10.20.3.10/api/v1" HEADERS = {"Authorization": "Bearer <api-token>"}

while True:

r = requests.get(f"{GATEWAY}/io/analog", headers=HEADERS,

verify="/etc/ssl/kx400-ca.pem", timeout=5) r.raise_for_status()

for ch in r.json()["channels"]:

print(f'{ch["name"]}: {ch["value"]:.3f} {ch["unit"]}') time.sleep(1)

The API is described in OpenAPI 3.1 format at https://<gateway>/api/v1/openapi.json. Rate limits apply: 20 requests per second per token.# 8 Maintenance

The gateway contains no user-serviceable parts and needs no routine maintenance apart from the checks below. The supercapacitor that buffers the real-time clock does not need replacement.

Table 6. Recommended maintenance schedule

<table>
<tbody>
<tr><td>Interval</td><td>Task</td></tr>
<tr><td>Monthly</td><td>Check SYS and BUS LEDs; review the alarm log</td></tr>
<tr><td>Every 6 months</td><td>Check terminal screws and cable strain relief; clean ventilation slots with dry compressed air</td></tr>
<tr><td>Yearly</td><td>Install current firmware; renew TLS certificates that expire within 90 days; test backup restore</td></tr>
</tbody>
</table>

## 8.1 Backup and restore

A backup contains the complete configuration, user accounts, certificates and pipeline definitions, but not logged data or container images. Create a backup before every firmware update.

Listing 5. Creating and restoring a backup

kx-cli backup create --encrypt --output /media/usb/kx400-$(date +%F).kxb kx-cli backup restore /media/usb/kx400-2026-07-01.kxb

## 8.2 Firmware update

- 1. Download and verify a. Download the .kxfw file and its .sig signature from the support portal. b. The gateway refuses images that are not signed by Kestrel.

- 2. Install a. Upload the file under System › Firmware, or run kx-cli fw install kx400-3.2.4.kxfw. b. The image is written to the inactive partition while the gateway keeps running.

- 3. Reboot and confirm a. The gateway restarts from the new partition (about 90 s). b. If it does not boot within 5 minutes it returns to the previous version automatically.

### CAUTION

Do not interrupt the power supply while SYS is blinking amber during the partition switch.# 9 Troubleshooting

Most problems can be identified from the LEDs and the system log (System › Logs, or kx-cli logs tail). Table 4 lists the most common symptoms. If the problem persists, create a diagnostic package with kxcli support bundle and send it to technical support.

Table 4. Troubleshooting

<table>
<tbody>
<tr><td>Symptom / LED</td><td>Possible cause</td><td>Remedy</td></tr>
<tr><td>PWR LED off</td><td>No supply voltage or reversed polarity</td><td>Measure 12–48 V DC at PWR1; check fuse F1 in cabinet</td></tr>
<tr><td>SYS LED blinking red (2 Hz)</td><td>Boot failure, corrupted firmware image</td><td>Hold RESET 15 s to boot recovery partition; reflash (section 8.2)</td></tr>
<tr><td>SYS LED amber</td><td>Storage above 90 % or temperature above 75 °C</td><td>Clear logs (kx-cli logs purge); improve cabinet ventilation</td></tr>
<tr><td>ETH link LED off</td><td>Cable or switch port fault</td><td>Replace cable; verify port speed is not forced to 10 Mb/s half duplex</td></tr>
<tr><td>Modbus timeouts (code</td><td>Wrong baud rate, missing</td><td>Match 19,200 8E1 on all devices; enable 120 Ω</td></tr>
<tr><td>0x0B)</td><td>termination</td><td>at both bus ends</td></tr>
<tr><td>MQTT broker refuses connection</td><td>Expired client certificate</td><td>Upload new certificate under Settings › Security › Certificates</td></tr>
<tr><td>Web UI not reachable</td><td>IP conflict or browser cache</td><td>Use service port 192.168.250.1; clear cache; check ip addr</td></tr>
<tr><td>LTE LED off (-LW)</td><td>SIM not detected or APN wrong</td><td>Reseat SIM; set APN in Network › Cellular; check signal ≥ −100 dBm</td></tr>
<tr><td>WARNING</td><td></td><td></td></tr>
<tr><td>A factory reset (RESET held backup first if the device</td><td>for 30 s) deletes all configuration, is still reachable.</td><td>certificates and user accounts. Create a</td></tr>
</tbody>
</table># 10 Support, warranty and regulatory information

 support@kestrel-automation.example  +1 937-555-0110 (Mon–Fri, 8:00–18:00 ET)  support.kestrel-automation.example

Kestrel Automation warrants the KX-400 against defects in material and workmanship for 36 months from the date of shipment. The warranty does not cover damage caused by improper installation, overvoltage, or operation outside the specified conditions. Local partners: Kestrel Automation GmbH (Stuttgart), Kestrel Automation Japan K.K. （ケストレル・オートメーション株式会社, 横浜市港北区新横浜 3-7-18）and Automatización Kestrel S.A. de C.V. (Monterrey).

## 10.1 FCC statement

This device complies with Part 15 of the FCC Rules. Operation is subject to the following two conditions:

- (1) this device may not cause harmful interference, and (2) this device must accept any interference received, including interference that may cause undesired operation. FCC ID: 2BKXA-KX400LW.

## 10.2 EU declaration of conformity

Kestrel Automation Inc. declares that the KX-400 series complies with Directives 2014/30/EU (EMC), 2014/53/EU (RED, -LW variant) and 2011/65/EU (RoHS). The full declaration is available on the support portal.

## 10.3 Disposal

Do not dispose of the device with household waste. In the EU, return it to a WEEE collection point; in the United States, use the Kestrel take-back program. The device contains a lithium-free supercapacitor and no batteries.# Appendix A Modbus register map

The gateway acts as a Modbus TCP server on port 502 (unit ID 1) and exposes its own I/O and diagnostics. Register addresses use the 5-digit convention: 3xxxx are input registers (function code 04), 4xxxx holding registers (function codes 03, 06, 16). FLOAT32 values use big-endian word order (ABCD).

Table 5. Modbus TCP register map (unit ID 1)

<table>
<tbody>
<tr><td>Address</td><td>Name</td><td>Type</td><td>Unit</td><td>Access</td></tr>
<tr><td>30001</td><td>Supply voltage PWR1</td><td>UINT16</td><td>0.01 V</td><td>R</td></tr>
<tr><td>30002</td><td>Supply voltage PWR2</td><td>UINT16</td><td>0.01 V</td><td>R</td></tr>
<tr><td>30003</td><td>Internal temperature</td><td>INT16</td><td>0.1 °C</td><td>R</td></tr>
<tr><td>30010–30017</td><td>Digital inputs DI1–DI8</td><td>BOOL ×8</td><td>—</td><td>R</td></tr>
<tr><td>30020–30027</td><td>Analog inputs AI1–AI4</td><td>FLOAT32 ×4</td><td>V or mA</td><td>R</td></tr>
<tr><td>30040</td><td>DI1 pulse counter</td><td>UINT32</td><td>pulses</td><td>R</td></tr>
<tr><td>40001–40004</td><td>Digital outputs DO1–DO4</td><td>BOOL ×4</td><td>—</td><td>R/W</td></tr>
<tr><td>40010</td><td>Watchdog timeout</td><td>UINT16</td><td>s</td><td>R/W</td></tr>
<tr><td>40020</td><td>Reset DI1 counter</td><td>UINT16 (write</td><td>—</td><td>0xA5A5) W</td></tr>
</tbody>
</table>

*Note*

Writing the watchdog register (40010) enables a fail-safe mode: if no write occurs within the timeout, all digital outputs switch off. A value of 0 disables the watchdog.

Kestrel Automation Inc. · 4180 Commerce Parkway · Dayton, OH 45414 · USA · Printed in USA · Doc. KX400-IOM-EN-C