FROM debian:bookworm AS isolate-build

WORKDIR /opt

RUN apt-get update && \
    apt-get -y install git gcc pkg-config make libcap-dev libsystemd-dev

RUN git clone https://github.com/ioi/isolate

WORKDIR /opt/isolate

RUN make isolate isolate-cg-keeper default.cf

FROM debian:bookworm 

RUN echo "deb http://deb.debian.org/debian testing main" >> /etc/apt/sources.list

RUN apt-get update && \
    apt-get -y install python3 gcc g++ rustc openjdk-17-jdk golang

RUN ln -sf /usr/bin/gcc /usr/bin/cc
RUN ln -sf /usr/lib/jvm/java-17-openjdk-*/bin/javac /usr/bin/javac
RUN ln -sf /usr/lib/jvm/java-17-openjdk-*/bin/java /usr/bin/java

COPY --from=isolate-build /opt/isolate/isolate /usr/local/bin/isolate
COPY --from=isolate-build /opt/isolate/isolate-cg-keeper /usr/local/bin/isolate-cg-keeper
COPY --from=isolate-build /opt/isolate/default.cf /usr/local/etc/isolate

WORKDIR /app

COPY ./target/release/kontestis-evaluator-v2 /app/evaluator

COPY ./.docker/* /app/docker/
RUN chmod +x /app/docker/*.sh

CMD ["/app/docker/entry.sh"]
