#!/bin/bash
# Full E2E test on two VMs with NFSv4 shared storage.
#
# Prerequisites:
#   - VM1 (controller): runs flockd, iptlb, git-sync
#   - VM2 (worker): runs podlet instances via systemd
#   - Both VMs mount /mnt/state from an NFSv4 server
#   - SSH key-based auth from controller -> worker
#   - Binaries installed at /usr/local/bin/ on both VMs
#   - Systemd units installed on both VMs
#
# Usage: ./e2e-vm.sh <controller_ip> <worker_ip>
set -euo pipefail

CONTROLLER="${1:?Usage: $0 <controller_ip> <worker_ip>}"
WORKER="${2:?}"

STATE_DIR="/mnt/state"
SPECS_DIR="${STATE_DIR}/specs"
NODES_FILE="/etc/flockd/nodes"

echo "=== not-k8s E2E VM Test ==="
echo "Controller: ${CONTROLLER}"
echo "Worker:     ${WORKER}"
echo ""

# -------------------------------------------------------------------
# 1. Set up shared state
# -------------------------------------------------------------------
echo "--- 1. Setting up shared state on NFS ---"
ssh "root@${CONTROLLER}" "
    mkdir -p ${SPECS_DIR}/prod ${STATE_DIR}/heartbeats ${STATE_DIR}/services
    echo '${CONTROLLER}' > ${NODES_FILE}
    echo '${WORKER}' >> ${NODES_FILE}
"
echo "State directory created on NFS"
echo ""

# -------------------------------------------------------------------
# 2. Deploy a web app spec
# -------------------------------------------------------------------
echo "--- 2. Creating spec: frontend (replicas: 2) ---"
ssh "root@${CONTROLLER}" "cat > ${SPECS_DIR}/prod/frontend.yaml" <<'EOF'
name: frontend
kind: Service
replicas: 2
image: nginx:alpine
cpu: 0.5
mem: "64Mi"
ports:
  - 80
health_check:
  tcp: 80
strategy:
  type: RollingUpdate
  drain_timeout: 30s
EOF
echo "Spec created"
echo ""

# -------------------------------------------------------------------
# 3. Start iptlb on controller
# -------------------------------------------------------------------
echo "--- 3. Starting iptlb on controller ---"
ssh "root@${CONTROLLER}" "
    systemctl stop iptlb 2>/dev/null || true
    iptlb \\
        --vip 10.0.0.100 \\
        --port 80 \\
        --backends-file ${STATE_DIR}/services/frontend.backend \\
        --scheduler rr \\
        --interval 2 &
"
echo "iptlb started"
echo ""

# -------------------------------------------------------------------
# 4. Start podlet-heartbeat timer on both VMs
# -------------------------------------------------------------------
echo "--- 4. Starting heartbeat timers ---"
ssh "root@${CONTROLLER}" "
    cp /usr/local/bin/podlet-heartbeat /usr/local/bin/ 2>/dev/null || true
    systemctl enable --now podlet-heartbeat.timer
"
ssh "root@${WORKER}" "
    cp /usr/local/bin/podlet-heartbeat /usr/local/bin/ 2>/dev/null || true
    systemctl enable --now podlet-heartbeat.timer
"
echo "Heartbeat timers started"
echo ""

# -------------------------------------------------------------------
# 5. Start flockd on controller
# -------------------------------------------------------------------
echo "--- 5. Starting flockd (reconciler) ---"
ssh "root@${CONTROLLER}" "
    systemctl stop flockd 2>/dev/null || true
    systemctl start flockd
    sleep 2
    systemctl status flockd --no-pager
"
echo ""

# -------------------------------------------------------------------
# 6. Wait for reconciliation
# -------------------------------------------------------------------
echo "--- 6. Waiting for reconciliation (30s) ---"
sleep 30
echo ""

# -------------------------------------------------------------------
# 7. Verify podlets are running on worker
# -------------------------------------------------------------------
echo "--- 7. Verifying podlet instances ---"
ssh "root@${WORKER}" "systemctl list-units 'podlet@*' --no-pager" || echo "  No podlet units found"
echo ""

# -------------------------------------------------------------------
# 8. Check health
# -------------------------------------------------------------------
echo "--- 8. Checking heartbeats ---"
ssh "root@${CONTROLLER}" "ls -la ${STATE_DIR}/heartbeats/ 2>/dev/null"
echo ""

# -------------------------------------------------------------------
# 9. Verify iptlb backends
# -------------------------------------------------------------------
echo "--- 9. Checking iptlb backends ---"
ssh "root@${CONTROLLER}" "cat ${STATE_DIR}/services/frontend.backend 2>/dev/null"
echo ""

# -------------------------------------------------------------------
# 10. Rolling update: change image tag
# -------------------------------------------------------------------
echo "--- 10. Rolling update (nginx:alpine -> nginx:1.25) ---"
ssh "root@${CONTROLLER}" "cat > ${SPECS_DIR}/prod/frontend.yaml" <<'EOF'
name: frontend
kind: Service
replicas: 2
image: nginx:1.25
cpu: 0.5
mem: "64Mi"
ports:
  - 80
health_check:
  tcp: 80
strategy:
  type: RollingUpdate
  drain_timeout: 30s
EOF

echo "Spec updated. Waiting for rolling update (60s)..."
sleep 60
echo ""

# -------------------------------------------------------------------
# 11. Verify rolling update completed
# -------------------------------------------------------------------
echo "--- 11. Post-update status ---"
ssh "root@${WORKER}" "systemctl list-units 'podlet@*' --no-pager"
echo ""
ssh "root@${CONTROLLER}" "cat ${STATE_DIR}/services/frontend.backend 2>/dev/null"
echo ""

# -------------------------------------------------------------------
# 12. Simulate node failure
# -------------------------------------------------------------------
echo "--- 12. Simulating worker node failure ---"
ssh "root@${CONTROLLER}" "
    # Mark worker as down by removing its heartbeat
    rm -f ${STATE_DIR}/heartbeats/${WORKER}
    sleep 5
"
echo "Worker heartbeat removed"
sleep 15
echo ""

# Check if flockd detected and rescheduled
echo "--- 13. Post-failure status ---"
ssh "root@${CONTROLLER}" "ls -la ${STATE_DIR}/heartbeats/ 2>/dev/null"
echo ""

# -------------------------------------------------------------------
# 13. Recover the worker
# -------------------------------------------------------------------
echo "--- 14. Worker recovery ---"
ssh "root@${WORKER}" "
    # Restart heartbeat
    /usr/local/bin/podlet-heartbeat
    sleep 10
"
echo "Worker recovered"
echo ""

echo "=== E2E VM test complete ==="
echo "Check controller logs: journalctl -u flockd"
echo "Check worker logs:   journalctl -u 'podlet@*'"
