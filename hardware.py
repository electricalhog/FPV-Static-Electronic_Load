import busio
import board

import displayio

displayio.release_displays()

import time

from pulseio import PWMOut
from digitalio import DigitalInOut

from adafruit_focaltouch import Adafruit_FocalTouch
from adafruit_ili9341 import ILI9341

i2c = board.I2C()
spi = board.SPI()

display_bus = displayio.FourWire(
    spi,
    command=board.D6,
    chip_select=board.D5,
    reset=board.D3,
    baudrate=1000_000_000,
)

display = ILI9341(display_bus, width=320, height=240)

touchscreen = Adafruit_FocalTouch(i2c)


def _refresh():
    return
