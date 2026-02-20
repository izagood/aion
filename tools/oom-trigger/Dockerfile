# OOM Trigger - Test container for triggering OOM kills
# Used to validate AION's OOM detection and remediation pipeline

FROM alpine:3.20

RUN apk add --no-cache stress-ng bash

COPY trigger.sh /trigger.sh
RUN chmod +x /trigger.sh

ENTRYPOINT ["/trigger.sh"]
