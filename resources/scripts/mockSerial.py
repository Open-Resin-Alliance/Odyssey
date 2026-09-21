#!/usr/bin/env python3
import os, pty, time

primary, secondary = pty.openpty()
s_name = os.ttyname(secondary)
m_name = os.ttyname(primary)

COMP_RESPOND_COMMANDS = [
    "g0","g1","move_plate","home_axis","dwell"
]
COMP_RESPONSE = bytes('Z_move_comp\r\n', 'utf-8')

STATUS_COMMAND = "status"
STATUS_DISCONNECT = bytes('Klipper state: Disconnect\r\n', 'utf-8')
STATUS_READY = bytes('Klipper state: Ready\r\n', 'utf-8')

print(f"Virtual Serial Port: {s_name}")

status_counter = 0

while True:
    # To read from the device
    time.sleep(1)
    serial_input = os.read(primary,1000).decode('utf-8')

    if serial_input:
        print(f"Received: {serial_input}")

    if any(command in serial_input.lower() for command in COMP_RESPOND_COMMANDS):
        os.write(primary,COMP_RESPONSE)
    elif STATUS_COMMAND in serial_input.lower():
        if status_counter<10:
            status_counter += 1
            os.write(primary, STATUS_DISCONNECT)
        else:
            os.write(primary, STATUS_READY)
