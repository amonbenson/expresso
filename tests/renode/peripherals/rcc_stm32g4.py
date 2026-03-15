# RCC stub for STM32G4 family (RM0440).
#
# Immediately asserts all oscillator and PLL ready flags when the
# corresponding enable bit is written, so that embassy-stm32's
# clock-init polling loops terminate instantly.
#
# Uses Renode's Python peripheral API (CamelCase: IsInit/IsRead/IsWrite,
# Offset, Value).

if request.IsInit:
    # CR reset value: HSION=1, HSIRDY=1 (bits 0 and 2 set)
    REGS = {0x00: (1 << 0) | (1 << 2)}

if request.IsWrite:
    val = request.Value
    offset = request.Offset
    if offset == 0x00:  # CR
        if val & (1 << 0):   val = val | (1 << 2)   # HSION  -> HSIRDY
        if val & (1 << 16):  val = val | (1 << 17)  # HSEON  -> HSERDY
        if val & (1 << 24):  val = val | (1 << 25)  # PLLON  -> PLLRDY
    elif offset == 0x08:  # CFGR: mirror SW[1:0] -> SWS[3:2]
        sw = val & 0x3
        val = (val & ~0xC) | (sw << 2)
    REGS[offset] = val

if request.IsRead:
    val = REGS.get(request.Offset, 0)
    if request.Offset == 0x00:
        if val & (1 << 0):   val = val | (1 << 2)
        if val & (1 << 16):  val = val | (1 << 17)
        if val & (1 << 24):  val = val | (1 << 25)
    elif request.Offset == 0x08:
        sw = val & 0x3
        val = (val & ~0xC) | (sw << 2)
    request.Value = val
