## CALDER HYDRO

# CH-6 Smart Irrigation Controller

Installation and User Guide for models CH-6, CH-6 Pro and CH-6-O (outdoor). Firmware 2.8 and later.

Automatic watering keeps borders healthy while using up to 40% less water than a hose.

## Before you start

Thank you for choosing Calder. Please read this guide fully before installing the controller and keep it somewhere safe. The installation of the controller itself is straightforward, but the wiring of valves in the garden may require some experience with outdoor electrical work. If in doubt, ask a competent installer; a list of Calder-approved installers is available at calderhydro.example/installers.# 1 Safety

### Warning

The plug-in transformer must be connected to a socket protected by a 30 mA residual current device (RCD). Do not install the indoor model outdoors or in a damp location such as an unheated garage with condensation.

### Warning

Never connect mains voltage (230 V) to any terminal of the controller. The valve outputs operate at 24 V AC only. Connecting mains voltage will destroy the controller and may cause fire.

- This appliance can be used by children aged from 8 years and above if they have been given supervision or instruction.

- Children shall not play with the appliance.

- Use only the transformer supplied (part no. CH-TX24).

- Isolate the water supply before working on valves or pipework.

- Fit a double check valve or an air gap to protect the drinking-water supply, as required by the Water Supply (Water Fittings) Regulations 1999.

### Note

Some water companies restrict the use of automatic watering systems during a hosepipe ban. Drip irrigation is often exempt. Check with your water supplier before use.

# 2 What is in the box

| • | CH-6 controller with wall-mounting plate |
| --- | --- |
| • | Plug-in transformer 230 V / 24 V AC with 2 m lead |
| • | Fixing kit: 3 screws and wall plugs |
| • | Terminal labels and wiring diagram card |
| • | This guide and a quick start card |

You will also need: a 3 mm flat screwdriver, a drill with a 6 mm masonry bit, multi-core irrigation cable, waterproof gel-filled connectors and a smartphone with the Calder Garden app (iOS 16 or Android 10 and later).# 3 Technical specification

Table 1. Technical specification

| Item | Specification |
| --- | --- |
| Model | CH-6 (6 zones), expandable to 18 zones with CH-EX6 modules |
| Supply | 230 V AC 50 Hz via plug-in transformer, 24 V AC 1.25 A output |
| Zone outputs | 24 V AC, 0.56 A per zone, 0.84 A total including master valve |
| Master valve / pump relay | 1 output, 24 V AC, 0.28 A |
| Sensor inputs | 2 inputs for rain, soil-moisture or flow sensors (dry contact or pulse) |
| Flow sensor | Pulse input up to 100 Hz, K-factor configurable |
| Wi-Fi | 802.11 b/g/n, 2.4 GHz, WPA2/WPA3 |
| Ethernet | 10/100 Mbit/s RJ45 (CH-6 Pro only) |
| Data backup | Programmes stored in flash memory; clock backed up for 72 h |
| Display | 2.4 in colour LCD with 5-key keypad |

The total current of all zones running at the same time must not exceed 0.84 A. Most solenoid valves draw 0.2 to 0.3 A, so up to two zones plus the master valve can run together. The controller prevents overload by queueing zones automatically.

## 3.1 Compatible valves and sensors

The CH-6 works with any normally closed 24 V AC solenoid valve. Latching (DC) valves designed for battery controllers are not compatible. Calder valves CV-25 and CV-32 are recommended because their solenoids draw only 0.19 A.

- Rain sensors: any normally closed switch-type sensor, for example the Calder RS-1 wireless rain sensor

- Soil-moisture sensors: Calder SM-2 (pulse) or third-party sensors with a dry-contact output

- Flow sensors: Calder FS-20 (K-factor 3.42) or any pulse sensor up to 100 Hz

- Pumps: via a 24 V AC coil relay or the Calder PSR-16 pump start relay

### Tip

When using a flow sensor, run the Learn flow function once after installation. The controller then records the normal flow of each zone and raises an alarm if the flow rises by more than 25%, which usually indicates a burst pipe or a missing nozzle.Table 1. Technical specification (continued)

| Item | Specification |
| --- | --- |
| Operating temperature | -10 to +50 °C |
| Enclosure | Indoor model IP20; outdoor model CH-6-O IP55, lockable |
| Dimensions | 190 × 150 × 48 mm (indoor); 240 × 210 × 95 mm (outdoor) |
| Weight | 0.62 kg (indoor); 1.4 kg (outdoor) |
| Wire size to valves | 0.5 to 1.5 mm², max. 300 m at 1.0 mm² |
| Standards | BS EN 60335-2-24, UKCA, CE, RED 2014/53/EU |

# 4 Installation

## 4.1 Mounting the controller

Choose a dry, frost-free location near a mains socket, for example inside a garage or utility room. The display should be at eye level and out of direct sunlight. The outdoor model CH-6-O can be mounted on an external wall; fit it under an overhang where possible.

1

2

3

Fix the wall plate

| a | Hold the plate level and mark the three holes. |
| --- | --- |
| b | Drill 6 mm holes, insert the wall plugs and fix the plate. |
| Hang | the controller |
| a | Hook the controller onto the top lugs. |
| b | Secure it with the screw underneath. |

Route the valve cable through the cable entry at the bottom and fit the cable gland.

Leave a drip loop in the cable below the controller so that water running down the cable cannot enter the enclosure. Keep the controller at least 1 m away from the router if you use Wi-Fi, but within range for a reliable connection: the app shows signal strength during set-up and should read at least two bars.

Figure 1. Tools needed for installation.## 4.2 Wiring the valves

Each valve solenoid has two wires. Connect one wire of every solenoid to the common wire

- (C) and the other to the zone terminal (1 to 6). Join cables in the valve box with gel-filled connectors only; ordinary terminal blocks corrode within a season.

1

2

3

4

Prepare the cable a Strip 7 mm of insulation from each core. b Use the colour code on the wiring card for your records. Connect at the controller a Push each core into its spring terminal until it clicks. b Connect the common core to terminal C.

Connect sensors (optional) a Rain sensor to S1 and COM; remove the factory jumper. b Flow sensor to S2 and COM, observing polarity. Plug in the transformer and switch on. The controller starts and shows the language menu.

### Note

If you use a pump instead of mains pressure, connect the pump start relay to terminal MV. Never connect the pump motor directly to the controller.

## 4.3 Example zone plan

Table 2. Example zone schedule for a domestic garden

| Zone | Area | Emitter | Flow (l/min) | Run time |
| --- | --- | --- | --- | --- |
| 1 | Front lawn | Pop-up rotors (4) | 22.0 | 18 min |
| 2 | Rear lawn | Pop-up rotors (6) | 31.5 | 20 min |
| 3 | Herbaceous border | Drip line 16 mm | 6.4 | 35 min |
| 4 | Vegetable beds | Micro-sprays | 11.8 | 15 min |
| 5 | Greenhouse | Drip emitters 2 l/h | 3.2 | 25 min |
| 6 | Patio containers | Drip emitters 4 l/h | 2.6 | 10 min |

Total water use for the schedule above is roughly 1,280 litres per watering day, or about 1.3 m³. At a typical metered tariff this costs less than three pounds per day of watering.# 5 Connecting to the app

| 1 | Download the Calder Garden app and create an account. |
| --- | --- |
| 2 | Tap Add controller and scan the QR code on the side of the CH-6. |
| 3 | Select your home Wi-Fi network (2.4 GHz) and enter the password. |
| 4 | Name your zones and choose the soil type and plant type for each one. |
| 5 | Enable Weather skip so that watering is reduced after rain. |

The CH-6 Pro also offers a local API for home-automation systems. The following commands show the controller status and start zone 3 for 10 minutes from a computer on the same network:

Listing 1. Local API commands

curl http://ch6-garden.local/api/status curl -X POST http://ch6-garden.local/api/zones/3/run \

-H 'X-Api-Key: <your-key>' -d 'minutes=10'

Advanced users can import a complete programme as a YAML file in the app under Settings > Import:

Listing 2. Programme file

controller: CH-6 season_adjust: 100 # per cent rain_delay_hours: 48 programmes:

A: start_times: ['05:30', '21:00'] days: [mon, wed, fri, sun] zones:

1: 18 2: 20

B: start_times: ['06:15'] days: every_day zones:

3: 35 5: 25 6: 10# 6 Programming at the controller

You can programme the CH-6 entirely from the keypad if you prefer not to use the app. The controller has three independent programmes (A, B and C), each with up to four start times per day. Zones within a programme run one after another; programmes that overlap are queued.

1

2

3

Set the clock a Press MENU, choose Date & time, set the values with the arrow keys. Create a programme a Choose Programmes > A > Start times.

b Set the watering days: weekdays, odd/even days or an interval. c Set the run time for each zone; 0 min means the zone is not used. Check and save a Press OK to save; the home screen shows the next scheduled start.

## 6.1 Seasonal adjustment

Seasonal adjustment changes all run times by a percentage, from 10% to 200%. In spring and autumn 60 to 80% is usually sufficient; in a heatwave you may increase it to 150%. With Weather skip enabled in the app, the controller calculates the adjustment automatically from local evapotranspiration data supplied by the Met Office.

## 6.2 Manual watering

Press the zone number key and OK to start a single zone for its programmed time, or hold OK for three seconds to run all zones in sequence. Press STOP at any time to cancel watering.

Figure 2. Communal grounds watered by a CH-6 Pro with flow monitoring.# 7 Maintenance

- Spring: open the main valve, run each zone for two minutes and check every sprinkler and drip line for leaks.

- Summer: clean the filter on the drip line every month; check that sprinklers are not blocked by growing plants.

- Autumn: reduce seasonal adjustment; at the first frost forecast, drain the pipework or blow it out with compressed air.

- Winter: switch the controller to Off but leave it powered so that the clock and firmware stay current.

### Caution

Water left in pipes, valves or the backflow preventer can freeze and split them. Damage caused by frost is not covered by the warranty.

## 7.1 Firmware updates

Controllers connected to Wi-Fi update themselves overnight between 2am and 4am when no watering is scheduled. You can check the installed version under Menu > About. Updates never change your programmes. If an update fails, the controller restarts with the previous version and tries again the following night.

# 8 Troubleshooting

Table 3. Troubleshooting Symptom Likely cause What to do

Display blank No mains power or faulty Check the socket; measure 24 V AC at

transformer terminals AC1/AC2

Zone does not open Broken wire or faulty solenoid Measure solenoid resistance: 20 to 60

ohms is normal

'Short' shown on a Damaged cable or water in a Inspect valve box; remake joints with zone joint gel-filled connectors Watering skipped Rain sensor active or weather Check sensor status on the home

skip screen; review the app log

App shows 'Offline' Wi-Fi password changed or Re-run Wi-Fi set-up; fit a Wi-Fi extender

router too far away

Unexpected high flow Burst pipe or missing nozzle Close the master valve; inspect the zone alarm shown in the alarm# 9 Warranty and support

Calder Hydro Ltd guarantees the CH-6 against faulty materials and workmanship for three years from the date of purchase, extended to five years if you register the product within 60 days. This does not affect your statutory rights.

| • | Online help: calderhydro.example/support |
| --- | --- |
| • | Email: help@calderhydro.example |
| • | Telephone: 01422 555 830 (Monday to Friday, 8.30am to 5pm) |
| • | Post: Calder Hydro Ltd, Unit 7, Mytholm Works, Hebden Bridge HX7 6DL, United |

Kingdom

## Declaration of conformity

Calder Hydro Ltd declares that the radio equipment type CH-6 is in compliance with the Radio Equipment Regulations 2017 and Directive 2014/53/EU. The full text of the declarations is available at calderhydro.example/doc.

## Disposal

This product must not be disposed of with household waste. Take it to your local household waste recycling centre or return it to the retailer under the WEEE take-back scheme.

Calder, Calder Hydro and Calder Garden are trademarks of Calder Hydro Ltd. Registered in England and Wales, company number 08555123. Issue 4, January 2026.