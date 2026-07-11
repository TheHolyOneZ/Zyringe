# Zyringe helper .so — self-contained Mono payload (no libmono link).
# Produces a single universal libzyringe.so that works across Mono flavors.

CC      ?= gcc
CFLAGS  ?= -shared -fPIC -Wall -Wextra -Wno-unused-parameter -O2 -g
LDFLAGS := -ldl -lpthread
TARGET  := libzyringe.so
SRC     := zyringe_helper.c

.PHONY: all clean verify

all: $(TARGET)

$(TARGET): $(SRC)
	$(CC) $(CFLAGS) -o $@ $(SRC) $(LDFLAGS)

# Sanity: confirm the payload only NEEDs libc (proves it is self-contained).
verify: $(TARGET)
	@echo "== NEEDED entries =="
	@readelf -d $(TARGET) | grep NEEDED || true

clean:
	rm -f $(TARGET)
