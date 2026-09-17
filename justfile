set shell := ["bash", "-cu"]

firmware_package := "inkpaper-x4-pro"
simulator_package := "inkpaper-simulator"
x4_pro_target := "xtensa-esp32s3-none-elf"

firmware_elf := "target/" + x4_pro_target + "/firmware/" + firmware_package 

default:
    @just --list

test:
    cargo nextest run

sim:
    cargo run -p {{simulator_package}}

x4-run:
    cargo +inkpaper-esp run -p {{firmware_package}} --target {{x4_pro_target}} --profile firmware


        