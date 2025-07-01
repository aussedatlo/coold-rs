#!/bin/bash

# Example usage of coold-rs CLI
# Make sure the daemon is running first: sudo ./target/release/coold-rs daemon

echo "=== Coold-rs CLI Examples ==="
echo

echo "1. Get current fan status:"
./target/release/coold-rs cli status
echo

echo "2. List all configured fans:"
./target/release/coold-rs cli list
echo

echo "3. Get configuration for a specific fan:"
./target/release/coold-rs cli get fan_1
echo

echo "4. Update fan curve for fan_1:"
./target/release/coold-rs cli update fan_1 "25:10,35:30,45:50,55:70,65:85,75:100"
echo

echo "5. Add a new fan:"
./target/release/coold-rs cli add \
  --sensor-name "coretemp" \
  --sensor-input "temp2_input" \
  --pwm-name "nct6775" \
  --pwm-input "pwm2" \
  "30:20,40:40,50:60,60:80,70:100"
echo

echo "6. Check status again to see the new fan:"
./target/release/coold-rs cli status
echo

echo "7. Remove the fan we just added:"
./target/release/coold-rs cli remove fan_2
echo

echo "8. Get the full configuration:"
./target/release/coold-rs cli config
echo

echo "=== CLI Examples Complete ===" 