#!/bin/bash
# End-to-end check of the Linux agent under systemd. Plan 09 · A.8 (the parts that need no desktop).
#
# Run as root on a TEST machine or VM (it installs and then removes the service):
#   cargo build -p procastinacioff-agent
#   sudo scripts/linux_service_check.sh target/debug/procastinacioff-agent [user-to-test-as]
#
# Without a profile the agent blocks nothing (FREE), so the machine stays usable throughout.
set -u
BIN=${1:?usage: linux_service_check.sh <agent binary> [test user]}
TEST_USER=${2:-$(id -nu 1000 2>/dev/null || echo nobody)}
REPO=$(cd "$(dirname "$0")/.." && pwd)
UNIT=/etc/systemd/system/procastinacioff-agent.service
FAIL=0
check() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; FAIL=1; fi; }

cleanup() {
  systemctl stop procastinacioff-agent 2>/dev/null
  rm -f "$UNIT" /etc/credstore.encrypted/procastinacioff.master
  rmdir /etc/credstore.encrypted 2>/dev/null
  rm -rf /usr/lib/procastinacioff /var/lib/procastinacioff
  systemctl daemon-reload
}
trap cleanup EXIT

install -D -m755 "$BIN" /usr/lib/procastinacioff/procastinacioff-agent
install -D -m644 "$REPO/packaging/linux/procastinacioff-agent.service" "$UNIT"
mkdir -p -m 0700 /etc/credstore.encrypted
head -c 32 /dev/urandom | systemd-creds encrypt --with-key=auto --name=procastinacioff.master - \
  /etc/credstore.encrypted/procastinacioff.master 2>/dev/null

check "unit is valid" "$(systemd-analyze verify "$UNIT" 2>&1 | grep -c .)" "0"
systemctl daemon-reload
timeout 60 systemctl start procastinacioff-agent
check "starts and notifies READY" "$(systemctl is-active procastinacioff-agent)" "active"
check "socket" "$(stat -c '%A %U' /run/procastinacioff/agent.sock)" "srw-rw-rw- root"
check "data folder" "$(stat -c '%a %U' /var/lib/procastinacioff)" "700 root"
check "secrets encrypted (nonce + key + tag)" "$(stat -c %s /var/lib/procastinacioff/secrets/db_key.bin)" "60"
check "database encrypted (SQLCipher)" "$(head -c 15 /var/lib/procastinacioff/core.db | grep -ac 'SQLite format')" "0"
check "a user cannot stop it" "$(runuser -u "$TEST_USER" -- systemctl stop procastinacioff-agent >/dev/null 2>&1; echo $?)" "1"
check "a user program is not answered" "$(runuser -u "$TEST_USER" -- timeout 5 python3 -c "
import socket; s=socket.socket(socket.AF_UNIX); s.connect('/run/procastinacioff/agent.sock')
s.sendall(b'{}\n'); s.settimeout(3)
try: print(len(s.recv(100)))
except Exception: print(0)" 2>/dev/null)" "0"

# A frozen agent must never freeze the machine: the watchdog kills it and execs go on.
PID=$(systemctl show -p MainPID --value procastinacioff-agent)
kill -STOP "$PID"
START=$(date +%s)
timeout 60 bash -c "while [ \"\$(systemctl show -p MainPID --value procastinacioff-agent)\" = \"$PID\" ]; do sleep 1; done"
check "frozen agent replaced within 15 s" "$(( $(date +%s) - START <= 15 ))" "1"
check "programs run again" "$(timeout 10 /usr/bin/true && echo yes)" "yes"
check "the new agent is up" "$(timeout 30 systemctl is-active procastinacioff-agent)" "active"

exit $FAIL
