#!/usr/bin/env bash
# Spike 4: the Logos chat module as a multisig room, driven through logosctl 0.3.2.
# Three sessions on the logos.test fleet, a group grown to three, then the removal probe.
set -u
cd "$(dirname "$0")"
export APPIMAGE_EXTRACT_AND_RUN=1
L=./logosctl-x86_64.AppImage
log(){ echo "[$(date +%H:%M:%S)] $*"; }
C(){ local who=$1; shift; $L --config-dir ./$who -j "$@" 2>/dev/null; }
R(){ jq -r '.result'; }
ID(){ jq -r 'if (.result|type)=="object" then (.result.value // .result.id // .result.conversation_id // (.result|tostring)) else .result end'; }

for who in alice bob carol; do
  $L --config-dir ./$who daemon stop >/dev/null 2>&1 || true
  mkdir -p $who/modules
  [ -d $who/modules/chat_module ] || cp -a modules/chat_module modules/delivery_module $who/modules/
  $L --config-dir ./$who daemon start --detach >/dev/null 2>&1 && log "$who daemon up" || { log "$who daemon failed"; exit 1; }
  log "$who load: $(C $who module load chat_module | jq -c '{status,deps:.dependencies_loaded}')"
  log "$who init: $(C $who call chat_module init 'json:{"delivery_preset":"logos.test"}' | jq -c '.result')"
done
T=$(date +%s)
for who in alice bob carol; do
  for i in $(seq 1 150); do st=$(C $who call chat_module status | jq -r '.result.delivery_state'); case "$st" in
    online) log "$who online, $(( $(date +%s) - T ))s after init"; break;;
    error) log "$who delivery error: $(C $who call chat_module status | jq -c .result)"; exit 1;; esac; sleep 2; done
  [ "$st" = online ] || { log "$who never came online, last state $st"; exit 1; }
done
for who in alice bob carol; do C $who call chat_module get_address | R > $who-addr.txt; log "$who address $(cat $who-addr.txt)"; done

log "T0 alice creates the group"; T0=$(date +%s)
GID=$(C alice call chat_module create_group_conversation "Conclave room" "spike" | ID); log "group id $GID"
log "alice adds bob: $(C alice call chat_module add_group_member "$GID" "$(cat bob-addr.txt)" | jq -c '.result')"
for i in $(seq 1 120); do if C bob call chat_module list_conversations | grep -q "$GID"; then log "bob sees the group $(( $(date +%s) - T0 ))s after creation"; break; fi; sleep 5; done
T1=$(date +%s); log "bob adds carol: $(C bob call chat_module add_group_member "$GID" "$(cat carol-addr.txt)" | jq -c '.result')"
for i in $(seq 1 120); do if C carol call chat_module list_conversations | grep -q "$GID"; then log "carol sees the group $(( $(date +%s) - T1 ))s after the add"; break; fi; sleep 5; done
log "roster as alice sees it: $(C alice call chat_module list_group_members "$GID" | jq -c '.result')"
log "roster as carol sees it: $(C carol call chat_module list_group_members "$GID" | jq -c '.result')"

log "message round trip"; T2=$(date +%s); C alice call chat_module send_message "$GID" "proposal 1: pay 10 to X" | jq -c '.result' >/dev/null
for i in $(seq 1 60); do if C carol call chat_module get_messages "$GID" | grep -q "proposal 1"; then log "carol received $(( $(date +%s) - T2 ))s after send"; break; fi; sleep 2; done
log "messages as carol sees them: $(C carol call chat_module get_messages "$GID" | jq -c '.result' | cut -c1-400)"

log "REMOVAL PROBE: methods exposed: $(C alice module show chat_module | jq -c '.methods | map(if type=="object" then .name else . end)')"
C alice module show chat_module | jq -r '.methods[] | if type=="object" then .name else . end' | grep -i -E "remove|kick|leave|evict|ban" || log "no remove, kick, leave, evict or ban method"
log "reconciliation by new group: alice creates group 2 without carol"; T3=$(date +%s)
G2=$(C alice call chat_module create_group_conversation "Conclave room v2" "after removal" | ID); C alice call chat_module add_group_member "$G2" "$(cat bob-addr.txt)" >/dev/null
for i in $(seq 1 120); do if C bob call chat_module list_conversations | grep -q "$G2"; then log "bob sees group 2 $(( $(date +%s) - T3 ))s after creation"; break; fi; sleep 5; done
log "does carol still read group 1 after being left out of group 2?"; C alice call chat_module send_message "$GID" "after removal" >/dev/null; sleep 30
if C carol call chat_module get_messages "$GID" | grep -q "after removal"; then log "YES, carol still reads the old group"; else log "no, carol did not receive the post-removal message within 30s"; fi
log "can alice delete the old conversation locally: $(C alice call chat_module delete_conversation "$GID" | jq -c '.result')"
sleep 10; C alice call chat_module send_message "$GID" "after delete" 2>/dev/null | jq -c '.result' | sed 's/^/[after delete] send as alice: /'
log "status: $(for w in alice bob carol; do echo -n "$w=$(C $w call chat_module status | jq -c '.result')  "; done)"
log "done"; for who in alice bob carol; do $L --config-dir ./$who daemon stop >/dev/null 2>&1; done
