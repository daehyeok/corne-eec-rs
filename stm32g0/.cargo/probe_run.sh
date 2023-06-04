#!/bin/bash

#pyocd write firmware more quickly.
if [[ -z "${PROBE}" ]]; then
  pyocd load --target stm32g0b1ketx  --format elf $1
  probe-run --no-flash --chip STM32G0B1KETx $1
else
  pyocd load --target stm32g0b1ketx  --probe "${PROBE}" --format elf $1
  probe-run --no-flash --chip STM32G0B1KETx --probe "${PROBE}" $1
fi
