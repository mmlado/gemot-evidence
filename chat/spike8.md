# A non-member watching a room

How we ran the observation in FINDINGS.md section 7, on 12 October 2026. It needs logosctl 0.3.2, chat module 0.3.0 and delivery module 0.3.0 installed into each session's `modules/` directory, and a network path to the logos.test fleet that does not block its DNS. Our capture is `logs/spike8-observer-events.jsonl`.

Four sessions: `alice`, `bob` and `carol` with the chat module, and `eve` with only the delivery module. In the commands, `L` is the logosctl binary and `C <session>` is `$L --config-dir ./<session> -j`.

1. Start a daemon per session: `$L --config-dir ./<session> daemon start --detach`.
2. Bring up eve as a plain delivery node and record everything it receives:

   ```
   C eve module load delivery_module
   C eve call delivery_module createNode '{"mode":"Core","preset":"logos.test"}'
   C eve call delivery_module start
   C eve call delivery_module getConnectionStatus      # repeat until Connected
   $L --config-dir ./eve -j watch delivery_module > eve-events.jsonl &
   ```

3. Bring the members online and note their addresses:

   ```
   C alice module load chat_module
   C alice call chat_module init 'json:{"delivery_preset":"logos.test"}'
   C alice call chat_module status                     # repeat until delivery_state is online
   C alice call chat_module get_address
   ```

   The same for bob and carol. One init failed once on the HTTP key registry and succeeded on retry.
4. Alice creates the group. Eve derives the room's content topic from the group id alone and subscribes:

   ```
   GID=$(C alice call chat_module create_group_conversation "room" "spike8" | jq -r .result.value)
   ADDR=$(python3 -c "import hashlib,sys; print(hashlib.blake2b(b'delivery_addr|'+sys.argv[1].encode(), digest_size=6).hexdigest())" "$GID")
   C eve call delivery_module subscribe "/logos-chat/1/$ADDR/proto"
   ```

5. Alice adds bob, then carol, waiting until each lists the group, then sends messages of 2, 48 and 640 characters 15 seconds apart with `send_message`.
6. Inbox check: eve subscribes to `/logos-chat/1/<blake2b, 4 bytes, of "InboxV2|delivery_address|" and carol's address>/proto`, alice creates a second group and adds carol. Carol joins and eve sees nothing on that topic.
7. Decode what eve saw: `python3 decode-capture.py eve-events.jsonl "$GID"`.

`spike4.sh` is the earlier three-session run that measured join and message latency and the missing remove call.
