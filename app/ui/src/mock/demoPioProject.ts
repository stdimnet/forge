/**
 * A small PlatformIO project for browser mode, opened with `?project=pio`: `blinky`, an
 * Arduino sketch for an ST Nucleo-F401RE and an ESP32 board with a Unity test for the native
 * platform, an OpenOCD Download & Run configuration and the STM32F401's SVD file for the
 * Peripherals view. The CForge mocks treat it like the C++ demo (`isCppDemo()`), with the
 * PlatformIO and embedded commands answered by `embeddedMock.ts`.
 */

export const pioDemoName = "blinky";

const INI = `; PlatformIO Project Configuration File
; https://docs.platformio.org/page/projectconf.html

[platformio]
default_envs = nucleo_f401re

[env]
framework = arduino
monitor_speed = 115200
lib_deps =
    bblanchon/ArduinoJson @ ^7.2.0

[common]
build_flags = -Wall -DBLINK_MS=250

[env:nucleo_f401re]
platform = ststm32
board = nucleo_f401re
build_flags = \${common.build_flags} -DLED_PIN=PA5
upload_protocol = stlink
debug_tool = stlink

[env:esp32dev]
platform = espressif32 @ 6.9.0
board = esp32dev
upload_port = /dev/ttyUSB0
monitor_speed = 921600
build_flags = \${common.build_flags} -DLED_PIN=2

[env:native]
platform = native
framework =
test_framework = unity
`;

const MAIN = `#include <Arduino.h>

#include "blink.h"

static Blinker blinker(LED_PIN, BLINK_MS);

void setup() {
    Serial.begin(115200);
    pinMode(LED_PIN, OUTPUT);
    Serial.println("blinky ready");
}

void loop() {
    if (blinker.tick(millis())) {
        digitalWrite(LED_PIN, blinker.on() ? HIGH : LOW);
        Serial.printf("led %s after %lu ms\\n", blinker.on() ? "on" : "off", millis());
    }
}
`;

const BLINK_H = `#pragma once

#include <stdint.h>

/// Toggles an LED every \`period\` milliseconds.
class Blinker {
public:
    Blinker(uint8_t pin, uint32_t period) : pin_(pin), period_(period) {}

    /// True when the LED changed at \`now\`.
    bool tick(uint32_t now);
    bool on() const { return on_; }
    uint8_t pin() const { return pin_; }

private:
    uint8_t pin_;
    uint32_t period_;
    uint32_t last_ = 0;
    bool on_ = false;
};
`;

const BLINK_CPP = `#include "blink.h"

bool Blinker::tick(uint32_t now) {
    if (now - last_ < period_) return false;
    last_ = now;
    on_ = !on_;
    return true;
}
`;

const TEST = `#include <unity.h>

#include "blink.h"

void test_toggles_after_the_period() {
    Blinker b(5, 250);
    TEST_ASSERT_FALSE(b.tick(100));
    TEST_ASSERT_TRUE(b.tick(250));
    TEST_ASSERT_TRUE(b.on());
}

void test_waits_for_the_next_period() {
    Blinker b(5, 250);
    b.tick(250);
    TEST_ASSERT_FALSE(b.tick(400));
    TEST_ASSERT_TRUE(b.tick(500));
    TEST_ASSERT_FALSE(b.on());
}

int main() {
    UNITY_BEGIN();
    RUN_TEST(test_toggles_after_the_period);
    RUN_TEST(test_waits_for_the_next_period);
    return UNITY_END();
}
`;

/** The SVD file of the Peripherals view (a trimmed STM32F401 description). */
export const PIO_SVD = `<?xml version="1.0" encoding="utf-8"?>
<device schemaVersion="1.3" xmlns:xs="http://www.w3.org/2001/XMLSchema-instance" xs:noNamespaceSchemaLocation="CMSIS-SVD.xsd">
  <vendor>STMicroelectronics</vendor>
  <name>STM32F401</name>
  <version>1.8</version>
  <description>STM32F401 (trimmed)</description>
  <cpu>
    <name>CM4</name>
    <revision>r0p1</revision>
    <endian>little</endian>
  </cpu>
  <addressUnitBits>8</addressUnitBits>
  <width>32</width>
  <size>0x20</size>
  <resetValue>0x0</resetValue>
  <resetMask>0xFFFFFFFF</resetMask>
  <peripherals>
    <peripheral>
      <name>GPIOA</name>
      <description>General-purpose I/Os</description>
      <groupName>GPIO</groupName>
      <baseAddress>0x40020000</baseAddress>
      <addressBlock>
        <offset>0x0</offset>
        <size>0x400</size>
        <usage>registers</usage>
      </addressBlock>
      <registers>
        <register>
          <name>MODER</name>
          <displayName>MODER</displayName>
          <description>GPIO port mode register</description>
          <addressOffset>0x0</addressOffset>
          <size>0x20</size>
          <access>read-write</access>
          <resetValue>0xA8000000</resetValue>
          <fields>
            <field>
              <name>MODER5</name>
              <description>Port x configuration bits (y = 0..15)</description>
              <bitOffset>10</bitOffset>
              <bitWidth>2</bitWidth>
              <enumeratedValues>
                <enumeratedValue><name>Input</name><description>Input mode (reset state)</description><value>0</value></enumeratedValue>
                <enumeratedValue><name>Output</name><description>General purpose output mode</description><value>1</value></enumeratedValue>
                <enumeratedValue><name>Alternate</name><description>Alternate function mode</description><value>2</value></enumeratedValue>
                <enumeratedValue><name>Analog</name><description>Analog mode</description><value>0b11</value></enumeratedValue>
              </enumeratedValues>
            </field>
            <field>
              <name>MODER0</name>
              <description>Port x configuration bits (y = 0..15)</description>
              <bitRange>[1:0]</bitRange>
            </field>
          </fields>
        </register>
        <register>
          <name>IDR</name>
          <description>GPIO port input data register</description>
          <addressOffset>0x10</addressOffset>
          <access>read-only</access>
          <resetValue>0x00000000</resetValue>
          <fields>
            <field>
              <name>IDR%s</name>
              <description>Port input data (y = 0..15)</description>
              <dim>16</dim>
              <dimIncrement>1</dimIncrement>
              <bitOffset>0</bitOffset>
              <bitWidth>1</bitWidth>
            </field>
          </fields>
        </register>
        <register>
          <name>ODR</name>
          <description>GPIO port output data register</description>
          <addressOffset>0x14</addressOffset>
          <resetValue>0x00000000</resetValue>
          <fields>
            <field>
              <name>ODR5</name>
              <description>Port output data (y = 5)</description>
              <lsb>5</lsb>
              <msb>5</msb>
            </field>
          </fields>
        </register>
        <register>
          <name>BSRR</name>
          <description>GPIO port bit set/reset register</description>
          <addressOffset>0x18</addressOffset>
          <access>write-only</access>
          <fields>
            <field><name>BR5</name><bitOffset>21</bitOffset><bitWidth>1</bitWidth></field>
            <field><name>BS5</name><bitOffset>5</bitOffset><bitWidth>1</bitWidth></field>
          </fields>
        </register>
      </registers>
    </peripheral>
    <peripheral derivedFrom="GPIOA">
      <name>GPIOB</name>
      <baseAddress>0x40020400</baseAddress>
    </peripheral>
    <peripheral>
      <name>RCC</name>
      <description>Reset and clock control</description>
      <baseAddress>0x40023800</baseAddress>
      <registers>
        <register>
          <name>CR</name>
          <description>clock control register</description>
          <addressOffset>0x0</addressOffset>
          <resetValue>0x00000083</resetValue>
          <fields>
            <field><name>HSION</name><description>Internal high-speed clock enable</description><bitOffset>0</bitOffset><bitWidth>1</bitWidth></field>
            <field><name>HSIRDY</name><description>Internal high-speed clock ready flag</description><bitOffset>1</bitOffset><bitWidth>1</bitWidth><access>read-only</access></field>
            <field><name>PLLON</name><description>Main PLL enable</description><bitOffset>24</bitOffset><bitWidth>1</bitWidth></field>
          </fields>
        </register>
        <register>
          <name>AHB1ENR</name>
          <description>AHB1 peripheral clock enable register</description>
          <addressOffset>0x30</addressOffset>
          <fields>
            <field><name>GPIOAEN</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth></field>
            <field><name>GPIOBEN</name><bitOffset>1</bitOffset><bitWidth>1</bitWidth></field>
          </fields>
        </register>
      </registers>
    </peripheral>
    <peripheral>
      <name>USART2</name>
      <description>Universal synchronous asynchronous receiver transmitter</description>
      <groupName>USART</groupName>
      <baseAddress>0x40004400</baseAddress>
      <size>0x10</size>
      <registers>
        <register>
          <name>SR</name>
          <description>Status register</description>
          <addressOffset>0x0</addressOffset>
          <resetValue>0x00C0</resetValue>
          <fields>
            <field><name>TXE</name><description>Transmit data register empty</description><bitOffset>7</bitOffset><bitWidth>1</bitWidth></field>
            <field><name>TC</name><description>Transmission complete</description><bitOffset>6</bitOffset><bitWidth>1</bitWidth></field>
            <field><name>RXNE</name><description>Read data register not empty</description><bitOffset>5</bitOffset><bitWidth>1</bitWidth></field>
          </fields>
        </register>
        <register>
          <name>DR</name>
          <description>Data register</description>
          <addressOffset>0x4</addressOffset>
          <fields>
            <field><name>DR</name><bitOffset>0</bitOffset><bitWidth>9</bitWidth></field>
          </fields>
        </register>
      </registers>
    </peripheral>
    <peripheral>
      <name>TIM2</name>
      <description>General purpose timers</description>
      <baseAddress>0x40000000</baseAddress>
      <registers>
        <register>
          <dim>4</dim>
          <dimIncrement>0x4</dimIncrement>
          <dimIndex>1-4</dimIndex>
          <name>CCR%s</name>
          <description>capture/compare register</description>
          <addressOffset>0x34</addressOffset>
          <fields>
            <field><name>CCR</name><bitOffset>0</bitOffset><bitWidth>32</bitWidth></field>
          </fields>
        </register>
        <cluster>
          <name>DMA</name>
          <description>DMA settings</description>
          <addressOffset>0x48</addressOffset>
          <register>
            <name>DCR</name>
            <description>DMA control register</description>
            <addressOffset>0x0</addressOffset>
            <size>16</size>
            <fields>
              <field><name>DBL</name><bitOffset>8</bitOffset><bitWidth>5</bitWidth></field>
              <field><name>DBA</name><bitOffset>0</bitOffset><bitWidth>5</bitWidth></field>
            </fields>
          </register>
          <register>
            <name>DMAR</name>
            <description>DMA address for full transfer</description>
            <addressOffset>0x4</addressOffset>
          </register>
        </cluster>
      </registers>
    </peripheral>
  </peripherals>
</device>
`;

export const pioDemoFiles: Record<string, string> = {
  "platformio.ini": INI,
  ".gitignore": ".pio/\n.vscode/\n",
  "README.md": "# blinky\n\nBlinks the user LED of a Nucleo-F401RE (or an ESP32 DevKit).\n\n```sh\npio run -e nucleo_f401re -t upload\npio test -e native\n```\n",
  "src/main.cpp": MAIN,
  "src/blink.cpp": BLINK_CPP,
  "include/blink.h": BLINK_H,
  "test/test_blink/test_main.cpp": TEST,
  "STM32F401.svd": PIO_SVD,
};
