import board
import busio
import time

i2c = busio.I2C(board.SCL, board.SDA)

received = bytearray(2)

while i2c.try_lock():
    pass

while True:
    for i in range(0xFFFF):
        low = i % 0x0100
        high = i // 0x100
        i2c.writeto_then_readfrom(0x04, bytes([high, low]), received)
        high, low = received
        print(hex(high * 0x0100 + low))
        time.sleep(0.1)
