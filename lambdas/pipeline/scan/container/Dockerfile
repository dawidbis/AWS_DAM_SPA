# Obraz Lambdy ze skanerem: ClamAV + binarka `scan` w Ruście, która sama
# implementuje Lambda Runtime API (lambda_runtime), więc nie potrzebuje
# obrazu bazowego AWS. Architektura x86_64: buduje się natywnie na runnerach
# GitHub bez emulacji (ADR 0014).
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends clamav clamav-daemon ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /var/lib/clamav

COPY freshclam.conf /etc/clamav/freshclam.conf
RUN freshclam --config-file=/etc/clamav/freshclam.conf \
 && chmod -R a+rX /var/lib/clamav \
 && ls -la /var/lib/clamav

COPY clamd.conf /etc/clamav/clamd.conf
COPY bootstrap /usr/local/bin/bootstrap

ENV CLAMD_CONFIG=/etc/clamav/clamd.conf
# Skaner nie potrzebuje roota: czyta bazę, pisze tylko do /tmp.
USER nobody
ENTRYPOINT ["/usr/local/bin/bootstrap"]
