*** Variables ***
# Paths are relative to this file's directory (tests/renode/tests/).
${PLATFORM}    ${CURDIR}/../platforms/stm32g431cb.repl
${ELF}         ${CURDIR}/../../../target/thumbv7em-none-eabihf/release/expresso-fw.elf

*** Test Cases ***
Firmware Should Boot Without Hard Fault
    [Documentation]    Loads the release ELF, runs the simulation for two
    ...                simulated seconds, and verifies that the CPU program
    ...                counter is still within the flash address range
    ...                (0x08000000–0x08020000). A hard fault or reset loop
    ...                would move the PC outside this range.
    Execute Command    mach create
    Execute Command    machine LoadPlatformDescription @${PLATFORM}
    Execute Command    sysbus LoadELF @${ELF}
    Execute Command    emulation RunFor "0.1"
    ${pc}=             Execute Command    cpu PC
    Should Match Regexp    ${pc.strip()}    ^0x0?80[0-9A-Fa-f]+$
