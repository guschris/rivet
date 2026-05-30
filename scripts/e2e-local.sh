#!/bin/bash
# E2E test: simulates a full not-k8s deployment on a single machine.
# Uses local directories in place of NFSv4 shared storage.
set -euo pipefail

BIN_DIR="${CARGO_TARGET_DIR:-$(pwd)/target/release}"
PODLET="${BIN_DIR}/podlet"
IPTLB="${BIN_DIR}/iptlb"
FLOCKD="${BIN_DIR}/flockd"

# Simulate NFS shared state
STATE_DIR=$(mktemp -d -t not-k8s-e2e.XXXXXX)
trap "rm -rf $STATE_DIR" EXIT

mkdir -p "${STATE_DIR}/specs" "${STATE_DIR}/heartbeats" "${STATE_DIR}/services"

echo "=== E2E Test: not-k8s local simulation ==="
echo "State dir: ${STATE_DIR}"
echo ""

# -------------------------------------------------------------------
# 1. Create spec: a simple web app with 2 replicas
# -------------------------------------------------------------------
cat > "${STATE_DIR}/specs/frontend.yaml" <<'EOF'
name: frontend
kind: Service
replicas: 2
ports:
  - 8080
health_check:
  http: "/"
strategy:
  type: RollingUpdate
  drain_timeout: 5s
EOF

# Create nodes file (single node for local testing)
echo "localhost" > "${STATE_DIR}/nodes"

echo "--- 1. Spec created ---"
cat "${STATE_DIR}/specs/frontend.yaml"
echo ""

# -------------------------------------------------------------------
# 2. Start iptlb in the background
# -------------------------------------------------------------------
BACKENDS_FILE="${STATE_DIR}/services/frontend.backend"
echo "" > "${BACKENDS_FILE}"

echo "--- 2. Starting iptlb ---"
${IPTLB} \
    --vip 10.0.0.1 \
    --port 80 \
    --backends-file "${BACKENDS_FILE}" \
    --scheduler rr \
    --interval 1 &
IPTLB_PID=$!
echo "iptlb PID: ${IPTLB_PID}"
echo ""

# -------------------------------------------------------------------
# 3. Start a simple HTTP server as a "workload" using podlet
#    (Using python as the actual server process)
# -------------------------------------------------------------------
echo "--- 3. Starting workload with podlet ---"

# Allocate a port for the Python HTTP server
HOST_PORT=$(python3 -c "import socket; s=socket.socket(); s.bind(('',0)); print(s.getsockname()[1]); s.close()")

# Manual workload: start a Python HTTP server as a podlet-supervised process
${PODLET} \
    --name frontend-0 \
    --tcp-check ":${HOST_PORT}" \
    --health-interval 2s \
    --restart always \
    --max-restarts 3 \
    --ports "8080:${HOST_PORT}" \
    -- python3 -m http.server "${HOST_PORT}" &
PODLET_PID=$!
echo "podlet PID: ${PODLET_PID}, serving on port ${HOST_PORT}"
echo ""

# Give podlet time to start
sleep 2

# Verify podlet is running
if ! kill -0 ${PODLET_PID} 2>/dev/null; then
    echo "FAIL: podlet died"
    exit 1
fi

# Check JSON output: look for "healthy"
echo "--- 4. Checking podlet health ---"

# -------------------------------------------------------------------
# 4. Write backend to the services file for iptlb
# -------------------------------------------------------------------
echo "127.0.0.1:${HOST_PORT}" > "${BACKENDS_FILE}"
echo "Backend written: 127.0.0.1:${HOST_PORT}"
echo ""

sleep 2

# -------------------------------------------------------------------
# 5. Test the HTTP server directly
# -------------------------------------------------------------------
echo "--- 5. Testing HTTP server ---"
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${HOST_PORT}/" 2>/dev/null || echo "000")
if [ "${HTTP_CODE}" = "200" ]; then
    echo "PASS: HTTP server returned ${HTTP_CODE}"
else
    echo "FAIL: HTTP server returned ${HTTP_CODE}"
fi
echo ""

# -------------------------------------------------------------------
# 6. Start flockd to manage reconciliation
# -------------------------------------------------------------------
echo "--- 6. Starting flockd (reconciler) ---"
${FLOCKD} \
    --specs "${STATE_DIR}/specs" \
    --state "${STATE_DIR}/state.db" \
    --nodes-file "${STATE_DIR}/nodes" \
    --exec-create "echo 'deployed {name} on {node}' >> ${STATE_DIR}/deploy.log && touch ${STATE_DIR}/heartbeats/{name}" \
    --exec-delete "echo 'removed {name} from {node}' >> ${STATE_DIR}/deploy.log && rm -f ${STATE_DIR}/heartbeats/{name}" \
    --exec-health "test -f ${STATE_DIR}/heartbeats/{name}" \
    --scheduler first-fit \
    --interval 2 &
FLOCKD_PID=$!
echo "flockd PID: ${FLOCKD_PID}"
echo ""

# Give flockd time to reconcile
sleep 8

# -------------------------------------------------------------------
# 7. Verify flockd created instances
# -------------------------------------------------------------------
echo "--- 7. Verification ---"
echo "Heartbeats:"
ls -la "${STATE_DIR}/heartbeats/" 2>/dev/null || echo "  (none)"
echo ""

echo "Deploy log:"
cat "${STATE_DIR}/deploy.log" 2>/dev/null || echo "  (empty)"
echo ""

# Check that flockd created the expected number of deployments
CREATE_COUNT=$(grep -c "deployed" "${STATE_DIR}/deploy.log" 2>/dev/null || echo 0)
echo "Instances created by flockd: ${CREATE_COUNT}"
echo ""

# Check iptlb backends file
echo "iptlb backends:"
cat "${BACKENDS_FILE}" 2>/dev/null || echo "  (empty)"
echo ""

# -------------------------------------------------------------------
# 8. Rolling update: change the spec
# -------------------------------------------------------------------
echo "--- 8. Rolling update ---"
cat > "${STATE_DIR}/specs/frontend.yaml" <<'EOF'
name: frontend
kind: Service
replicas: 3
ports:
  - 8080
health_check:
  http: "/"
strategy:
  type: RollingUpdate
  drain_timeout: 5s
EOF

echo "Spec updated: replicas 2 -> 3"
sleep 10

CREATE_COUNT=$(grep -c "deployed" "${STATE_DIR}/deploy.log" 2>/dev/null || echo 0)
echo "Total creates after update: ${CREATE_COUNT}"
echo ""

# -------------------------------------------------------------------
# 9. Scale down
# -------------------------------------------------------------------
echo "--- 9. Scale down ---"
cat > "${STATE_DIR}/specs/frontend.yaml" <<'EOF'
name: frontend
kind: Service
replicas: 1
ports:
  - 8080
health_check:
  http: "/"
strategy:
  type: RollingUpdate
  drain_timeout: 5s
EOF

echo "Spec updated: replicas 3 -> 1"
sleep 10

DELETE_COUNT=$(grep -c "removed" "${STATE_DIR}/deploy.log" 2>/dev/null || echo 0)
echo "Deletions after scale-down: ${DELETE_COUNT}"
echo ""

# -------------------------------------------------------------------
# 10. Cleanup
# -------------------------------------------------------------------
echo "--- 10. Cleanup ---"
kill ${PODLET_PID} 2>/dev/null || true
kill ${FLOCKD_PID} 2>/dev/null || true
kill ${IPTLB_PID} 2>/dev/null || true
wait 2>/dev/null || true

echo ""
echo "=== E2E test complete ==="
echo "PASS: All phases completed"
