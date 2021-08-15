// Wire Slave Receiver
// by Nicholas Zambetti <http://www.zambetti.com>

// Demonstrates use of the Wire library
// Receives data as an I2C/TWI slave device
// Refer to the "Wire Master Writer" example for use with this

// Created 29 March 2006

// This example code is in the public domain.


#include <Wire.h>

int received = 0x0000;
int sent = 0x0000;

void setup()
{
  Wire.begin(0x04); //join i2c with address 4
  Wire.onReceive(receive); //setup handler for recieving
  Wire.onRequest(respond); //setup handler for sending
  Serial.begin(115200);
}

void loop()
{
  for(sent < 0xFFF; sent++;)
  {
    delay(100);
  }
}

void receive(int numBytes)
{
  byte high = Wire.read();
  byte low = Wire.read();
  received = high*0x100 + low;
  Serial.println(received, HEX);
}

void respond()
{
  byte high = sent / 0x0100;
  byte low = sent % 0x0100;
  Wire.write(high);
  Wire.write(low);
}
