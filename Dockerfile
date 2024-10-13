FROM docker.io/ubuntu:24.04
ARG TARGETARCH

RUN mkdir -p /opt/carousel
WORKDIR /opt/carousel
COPY ./tools/target_arch.sh /opt/carousel
COPY ./docker/entrypoint.sh /opt/carousel

RUN --mount=type=bind,target=/context \
 cp /context/target/$(/opt/carousel/target_arch.sh)/release/carousel /opt/carousel/carousel
USER 10000
CMD ["/opt/carousel/entrypoint.sh"]
EXPOSE 8443
