FROM golang:1.24-alpine AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY *.go ./
RUN CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /out/nooki-relay .

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /out/nooki-relay /nooki-relay
USER nonroot:nonroot
ENTRYPOINT ["/nooki-relay"]
