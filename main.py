import board
import busio
import time

i2c = busio.I2C(board.SCL, board.SDA)

while i2c.try_lock():
    pass

while True:
    for i in range(0xFFFF):
        low = i % 0x0100
        high = i // 0x100
        i2c.writeto(0x04, bytes([high, low]))
        time.sleep(0.01)
