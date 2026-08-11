package main

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
)

func TestMinecraftHandshakeKeepsOriginalBytes(t *testing.T) {
	original := makeHandshake("Demo.NoOkI.Example\x00FML3")
	host, captured, err := readMinecraftHandshake(bytes.NewReader(original))
	if err != nil {
		t.Fatal(err)
	}
	if host != "Demo.NoOkI.Example\x00FML3" {
		t.Fatalf("unexpected host %q", host)
	}
	if !bytes.Equal(captured, original) {
		t.Fatal("handshake bytes changed")
	}
	if normalized := normalizeMinecraftHost(host); normalized != "demo.nooki.example" {
		t.Fatalf("unexpected normalized host %q", normalized)
	}
}

func TestAuthenticationProofAndPerStartRoute(t *testing.T) {
	public, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	nonce := randomBytes(32)
	nonceText := base64.RawURLEncoding.EncodeToString(nonce)
	serverID := "server-12345678"
	routeToken := base64.RawURLEncoding.EncodeToString(randomBytes(18))
	auth := controlMessage{
		Type:       "authenticate",
		PublicKey:  base64.RawURLEncoding.EncodeToString(public),
		ServerID:   serverID,
		RouteToken: routeToken,
		Signature:  base64.RawURLEncoding.EncodeToString(ed25519.Sign(private, authenticationMessage(nonceText, serverID, routeToken, ""))),
	}
	verified, deviceID, err := verifyAuthentication(nonce, auth)
	if err != nil || deviceID == "" || !bytes.Equal(public, verified) {
		t.Fatalf("valid proof rejected: %v", err)
	}
	cfg := config{publicBase: "relay.example", labelSecret: bytes.Repeat([]byte{7}, 32)}
	first := routeHost(cfg, public, serverID, routeToken, "")
	second := routeHost(cfg, public, serverID, routeToken, "")
	changed := routeHost(cfg, public, serverID, base64.RawURLEncoding.EncodeToString(randomBytes(18)), "")
	if first != second || first == changed {
		t.Fatal("route labels must be stable for one start and change for the next")
	}
}

func TestAuthenticationRejectsWrongServer(t *testing.T) {
	public, private, _ := ed25519.GenerateKey(rand.Reader)
	nonce := randomBytes(32)
	nonceText := base64.RawURLEncoding.EncodeToString(nonce)
	routeToken := base64.RawURLEncoding.EncodeToString(randomBytes(18))
	auth := controlMessage{
		Type:       "authenticate",
		PublicKey:  base64.RawURLEncoding.EncodeToString(public),
		ServerID:   "server-12345678",
		RouteToken: routeToken,
		Signature:  base64.RawURLEncoding.EncodeToString(ed25519.Sign(private, authenticationMessage(nonceText, "different", routeToken, ""))),
	}
	if _, _, err := verifyAuthentication(nonce, auth); err == nil {
		t.Fatal("invalid proof was accepted")
	}
}

func TestEndToEndMinecraftTunnel(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	r := &relay{
		config: config{publicBase: "relay.example", labelSecret: bytes.Repeat([]byte{9}, 32)},
		byHost: make(map[string]*controlSession),
		byID:   make(map[string]*pendingConnection),
	}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /v1/control", r.control)
	mux.HandleFunc("GET /v1/data/{connection}", r.data)
	api := httptest.NewServer(mux)
	defer api.Close()
	websocketBase := "ws" + strings.TrimPrefix(api.URL, "http")

	control, _, err := websocket.Dial(ctx, websocketBase+"/v1/control", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer control.CloseNow()
	readMessage := func() controlMessage {
		_, payload, readErr := control.Read(ctx)
		if readErr != nil {
			t.Fatal(readErr)
		}
		var message controlMessage
		if err := json.Unmarshal(payload, &message); err != nil {
			t.Fatal(err)
		}
		return message
	}
	challenge := readMessage()
	public, private, _ := ed25519.GenerateKey(rand.Reader)
	serverID := "server-end-to-end"
	routeToken := base64.RawURLEncoding.EncodeToString(randomBytes(18))
	auth := controlMessage{
		Type:       "authenticate",
		PublicKey:  base64.RawURLEncoding.EncodeToString(public),
		Signature:  base64.RawURLEncoding.EncodeToString(ed25519.Sign(private, authenticationMessage(challenge.Nonce, serverID, routeToken, ""))),
		ServerID:   serverID,
		ServerName: "Tunnel test",
		RouteToken: routeToken,
	}
	payload, _ := json.Marshal(auth)
	if err := control.Write(ctx, websocket.MessageText, payload); err != nil {
		t.Fatal(err)
	}
	ready := readMessage()
	if ready.Type != "ready" {
		t.Fatalf("relay was not ready: %+v", ready)
	}

	minecraft, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer minecraft.Close()
	go r.acceptPlayers(minecraft)
	target, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer target.Close()
	errors := make(chan error, 2)

	go func() {
		incoming := readMessage()
		if incoming.Type != "incoming" {
			errors <- fmt.Errorf("unexpected control message: %s", incoming.Type)
			return
		}
		dataURL := fmt.Sprintf("%s/v1/data/%s?token=%s", websocketBase, incoming.Connection, incoming.Token)
		dataSocket, _, err := websocket.Dial(ctx, dataURL, nil)
		if err != nil {
			errors <- err
			return
		}
		tunnel := websocket.NetConn(ctx, dataSocket, websocket.MessageBinary)
		local, err := net.Dial("tcp", target.Addr().String())
		if err != nil {
			errors <- err
			return
		}
		proxyConnections(tunnel, local)
	}()

	go func() {
		connection, err := target.Accept()
		if err != nil {
			errors <- err
			return
		}
		defer connection.Close()
		host, _, err := readMinecraftHandshake(connection)
		if err != nil {
			errors <- err
			return
		}
		if host != ready.Address {
			errors <- fmt.Errorf("wrong routed host: %s", host)
			return
		}
		ping := make([]byte, 4)
		if _, err := io.ReadFull(connection, ping); err != nil {
			errors <- err
			return
		}
		if string(ping) != "ping" {
			errors <- fmt.Errorf("wrong player payload: %q", ping)
			return
		}
		_, err = connection.Write([]byte("pong"))
		errors <- err
	}()

	player, err := net.Dial("tcp", minecraft.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	defer player.Close()
	_ = player.SetDeadline(time.Now().Add(5 * time.Second))
	if _, err := player.Write(append(makeHandshake(ready.Address), []byte("ping")...)); err != nil {
		t.Fatal(err)
	}
	pong := make([]byte, 4)
	if _, err := io.ReadFull(player, pong); err != nil {
		t.Fatal(err)
	}
	if string(pong) != "pong" {
		t.Fatalf("wrong tunnel response: %q", pong)
	}
	if err := <-errors; err != nil {
		t.Fatal(err)
	}
}

func TestVanityReservationsPersistAndRejectOtherOwners(t *testing.T) {
	path := t.TempDir() + "/vanity.json"
	r := &relay{
		config:        config{vanityFile: path},
		vanityByName:  make(map[string]string),
		vanityByOwner: make(map[string]string),
	}
	if err := r.claimVanity("owner-one", "parkour"); err != nil {
		t.Fatal(err)
	}
	if err := r.claimVanity("owner-two", "parkour"); err == nil {
		t.Fatal("another owner claimed a reserved vanity name")
	}
	byName, byOwner, err := loadVanityReservations(path)
	if err != nil || byName["parkour"] != "owner-one" || byOwner["owner-one"] != "parkour" {
		t.Fatalf("reservation was not persisted: %v", err)
	}
}
