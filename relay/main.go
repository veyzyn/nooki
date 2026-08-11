package main

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base32"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/coder/websocket"
)

const (
	protocolName       = "nooki-relay-v2"
	maxHandshakeBytes  = 4096
	maxConnections     = 20
	dataConnectTimeout = 15 * time.Second
)

type config struct {
	httpAddress      string
	adminAddress     string
	minecraftAddress string
	publicBase       string
	labelSecret      []byte
	adminToken       string
	vanityFile       string
	accessFile       string
}

type relay struct {
	config        config
	routes        sync.RWMutex
	byHost        map[string]*controlSession
	byDevice      map[string]*controlSession
	pending       sync.Mutex
	byID          map[string]*pendingConnection
	vanity        sync.Mutex
	vanityByName  map[string]string
	vanityByOwner map[string]string
	access        *accessStore
}

type controlSession struct {
	ws       *websocket.Conn
	host     string
	deviceID string
	serverID string
	active   atomic.Int32
	writeMu  sync.Mutex
}

type pendingConnection struct {
	token   []byte
	created time.Time
	ready   chan net.Conn
}

type controlMessage struct {
	Type       string `json:"type"`
	Nonce      string `json:"nonce,omitempty"`
	PublicKey  string `json:"publicKey,omitempty"`
	Signature  string `json:"signature,omitempty"`
	ServerID   string `json:"serverId,omitempty"`
	ServerName string `json:"serverName,omitempty"`
	RouteToken string `json:"routeToken,omitempty"`
	Vanity     string `json:"vanity,omitempty"`
	Address    string `json:"address,omitempty"`
	DeviceID   string `json:"deviceId,omitempty"`
	Connection string `json:"connectionId,omitempty"`
	Token      string `json:"token,omitempty"`
	Message    string `json:"message,omitempty"`
}

func main() {
	cfg, err := loadConfig()
	if err != nil {
		slog.Error("invalid configuration", "error", err)
		os.Exit(1)
	}
	vanityByName, vanityByOwner, err := loadVanityReservations(cfg.vanityFile)
	if err != nil {
		slog.Error("could not load vanity reservations", "error", err)
		os.Exit(1)
	}
	access, err := loadAccessStore(cfg.accessFile, cfg.labelSecret)
	if err != nil {
		slog.Error("could not load relay access database", "error", err)
		os.Exit(1)
	}
	r := &relay{
		config:        cfg,
		byHost:        make(map[string]*controlSession),
		byDevice:      make(map[string]*controlSession),
		byID:          make(map[string]*pendingConnection),
		vanityByName:  vanityByName,
		vanityByOwner: vanityByOwner,
		access:        access,
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", r.health)
	mux.HandleFunc("POST /v1/activate", r.activate)
	mux.HandleFunc("GET /v1/control", r.control)
	mux.HandleFunc("GET /v1/data/{connection}", r.data)
	server := &http.Server{
		Addr:              cfg.httpAddress,
		Handler:           securityHeaders(mux),
		ReadHeaderTimeout: 5 * time.Second,
		IdleTimeout:       90 * time.Second,
	}
	adminMux := http.NewServeMux()
	adminMux.HandleFunc("POST /v1/activation-codes", r.createActivationCodes)
	adminServer := &http.Server{
		Addr:              cfg.adminAddress,
		Handler:           securityHeaders(adminMux),
		ReadHeaderTimeout: 5 * time.Second,
		IdleTimeout:       30 * time.Second,
	}

	listener, err := net.Listen("tcp", cfg.minecraftAddress)
	if err != nil {
		slog.Error("could not listen for Minecraft", "address", cfg.minecraftAddress, "error", err)
		os.Exit(1)
	}
	go r.acceptPlayers(listener)
	go r.expirePending()
	go func() {
		slog.Info("relay API listening", "address", cfg.httpAddress)
		if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			slog.Error("relay API stopped", "error", err)
			os.Exit(1)
		}
	}()
	go func() {
		slog.Info("relay admin API listening", "address", cfg.adminAddress)
		if err := adminServer.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			slog.Error("relay admin API stopped", "error", err)
			os.Exit(1)
		}
	}()

	slog.Info("Minecraft relay listening", "address", cfg.minecraftAddress, "publicBase", cfg.publicBase)
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, syscall.SIGINT, syscall.SIGTERM)
	<-stop
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_ = server.Shutdown(ctx)
	_ = adminServer.Shutdown(ctx)
	_ = listener.Close()
}

func loadConfig() (config, error) {
	cfg := config{
		httpAddress:      envOr("HTTP_ADDRESS", "127.0.0.1:7000"),
		adminAddress:     envOr("ADMIN_ADDRESS", "127.0.0.1:7001"),
		minecraftAddress: envOr("MINECRAFT_ADDRESS", "0.0.0.0:25565"),
		publicBase:       strings.ToLower(strings.TrimSuffix(os.Getenv("PUBLIC_BASE"), ".")),
		vanityFile:       envOr("VANITY_FILE", "/data/vanity.json"),
		accessFile:       envOr("ACCESS_FILE", "/data/access.json"),
		adminToken:       os.Getenv("ADMIN_TOKEN"),
	}
	if cfg.publicBase == "" {
		return cfg, errors.New("PUBLIC_BASE is required")
	}
	secret, err := base64.RawURLEncoding.DecodeString(os.Getenv("LABEL_SECRET"))
	if err != nil || len(secret) < 32 {
		return cfg, errors.New("LABEL_SECRET must be at least 32 random base64url bytes")
	}
	cfg.labelSecret = secret
	if len(cfg.adminToken) < 32 {
		return cfg, errors.New("ADMIN_TOKEN must be at least 32 characters")
	}
	return cfg, nil
}

func envOr(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}

func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("Cache-Control", "no-store")
		next.ServeHTTP(w, r)
	})
}

func (r *relay) health(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	_, _ = io.WriteString(w, `{"ok":true}`)
}

func (r *relay) control(w http.ResponseWriter, request *http.Request) {
	ws, err := websocket.Accept(w, request, &websocket.AcceptOptions{CompressionMode: websocket.CompressionDisabled})
	if err != nil {
		return
	}
	defer ws.Close(websocket.StatusNormalClosure, "control closed")
	ws.SetReadLimit(16 * 1024)
	ctx := request.Context()

	nonce := randomBytes(32)
	challenge := controlMessage{Type: "challenge", Nonce: base64.RawURLEncoding.EncodeToString(nonce)}
	if err := writeJSON(ctx, ws, challenge); err != nil {
		return
	}
	_, payload, err := ws.Read(ctx)
	if err != nil {
		return
	}
	var auth controlMessage
	if json.Unmarshal(payload, &auth) != nil || auth.Type != "authenticate" {
		_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: "invalid authentication message"})
		return
	}
	publicKey, deviceID, err := verifyAuthentication(nonce, auth)
	if err != nil {
		_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: "identity proof was rejected"})
		return
	}
	if r.access != nil {
		if _, err := r.access.authorize(publicKey); err != nil {
			_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: err.Error()})
			return
		}
	}
	if len(auth.ServerID) < 8 || len(auth.ServerID) > 128 {
		_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: "invalid server identifier"})
		return
	}
	if token, err := base64.RawURLEncoding.DecodeString(auth.RouteToken); err != nil || len(token) != 18 {
		_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: "invalid route token"})
		return
	}
	vanity, err := normalizeVanity(auth.Vanity)
	if err != nil {
		_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: err.Error()})
		return
	}
	owner := routeOwner(publicKey, auth.ServerID)
	if err := r.claimVanity(owner, vanity); err != nil {
		_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: err.Error()})
		return
	}
	host := routeHost(r.config, publicKey, auth.ServerID, auth.RouteToken, vanity)
	session := &controlSession{ws: ws, host: host, deviceID: deviceID, serverID: auth.ServerID}
	old, err := r.registerRoute(session)
	if err != nil {
		_ = writeJSON(ctx, ws, controlMessage{Type: "error", Message: err.Error()})
		return
	}
	if old != nil {
		_ = old.ws.Close(websocket.StatusPolicyViolation, "route replaced by a new connection")
	}
	defer r.unregisterRoute(session)
	if err := session.send(ctx, controlMessage{Type: "ready", Address: host, DeviceID: deviceID}); err != nil {
		return
	}
	slog.Info("share connected", "host", host, "device", deviceID, "server", auth.ServerName)

	for {
		if _, _, err := ws.Read(ctx); err != nil {
			break
		}
	}
}

func verifyAuthentication(nonce []byte, auth controlMessage) ([]byte, string, error) {
	publicKey, err := base64.RawURLEncoding.DecodeString(auth.PublicKey)
	if err != nil || len(publicKey) != ed25519.PublicKeySize {
		return nil, "", errors.New("invalid public key")
	}
	signature, err := base64.RawURLEncoding.DecodeString(auth.Signature)
	if err != nil || len(signature) != ed25519.SignatureSize {
		return nil, "", errors.New("invalid signature")
	}
	message := authenticationMessage(
		base64.RawURLEncoding.EncodeToString(nonce),
		auth.ServerID,
		auth.RouteToken,
		auth.Vanity,
	)
	if !ed25519.Verify(ed25519.PublicKey(publicKey), message, signature) {
		return nil, "", errors.New("signature verification failed")
	}
	hash := sha256.Sum256(publicKey)
	return publicKey, hex.EncodeToString(hash[:8]), nil
}

func authenticationMessage(nonce, serverID, routeToken, vanity string) []byte {
	return []byte(protocolName + "\n" + nonce + "\n" + serverID + "\n" + routeToken + "\n" + vanity)
}

func routeHost(cfg config, publicKey []byte, serverID, routeToken, vanity string) string {
	if vanity != "" {
		return vanity + "." + cfg.publicBase
	}
	mac := hmac.New(sha256.New, cfg.labelSecret)
	_, _ = mac.Write(publicKey)
	_, _ = mac.Write([]byte{0})
	_, _ = mac.Write([]byte(serverID))
	_, _ = mac.Write([]byte{0})
	_, _ = mac.Write([]byte(routeToken))
	label := strings.ToLower(base32.StdEncoding.WithPadding(base32.NoPadding).EncodeToString(mac.Sum(nil))[:10])
	return label + "." + cfg.publicBase
}

func routeOwner(publicKey []byte, serverID string) string {
	hash := sha256.New()
	_, _ = hash.Write(publicKey)
	_, _ = hash.Write([]byte{0})
	_, _ = hash.Write([]byte(serverID))
	return hex.EncodeToString(hash.Sum(nil))
}

func normalizeVanity(value string) (string, error) {
	value = strings.ToLower(strings.TrimSpace(value))
	if value == "" {
		return "", nil
	}
	if len(value) < 3 || len(value) > 32 || value[0] == '-' || value[len(value)-1] == '-' {
		return "", errors.New("use 3–32 letters, numbers, or hyphens for the vanity address")
	}
	for _, character := range value {
		if (character < 'a' || character > 'z') && (character < '0' || character > '9') && character != '-' {
			return "", errors.New("use 3–32 letters, numbers, or hyphens for the vanity address")
		}
	}
	return value, nil
}

func loadVanityReservations(path string) (map[string]string, map[string]string, error) {
	byName := make(map[string]string)
	byOwner := make(map[string]string)
	payload, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return byName, byOwner, nil
	}
	if err != nil {
		return nil, nil, err
	}
	if err := json.Unmarshal(payload, &byName); err != nil {
		return nil, nil, err
	}
	for name, owner := range byName {
		if normalized, err := normalizeVanity(name); err != nil || normalized != name || owner == "" {
			return nil, nil, fmt.Errorf("invalid vanity reservation %q", name)
		}
		byOwner[owner] = name
	}
	return byName, byOwner, nil
}

func (r *relay) claimVanity(owner, name string) error {
	r.vanity.Lock()
	defer r.vanity.Unlock()

	if claimedBy := r.vanityByName[name]; name != "" && claimedBy != "" && claimedBy != owner {
		return errors.New("that vanity address is already taken")
	}
	previous := r.vanityByOwner[owner]
	if previous == name {
		return nil
	}
	if previous != "" {
		delete(r.vanityByName, previous)
	}
	if name == "" {
		delete(r.vanityByOwner, owner)
	} else {
		r.vanityByName[name] = owner
		r.vanityByOwner[owner] = name
	}
	if err := persistVanityReservations(r.config.vanityFile, r.vanityByName); err != nil {
		if name != "" {
			delete(r.vanityByName, name)
			delete(r.vanityByOwner, owner)
		}
		if previous != "" {
			r.vanityByName[previous] = owner
			r.vanityByOwner[owner] = previous
		}
		return fmt.Errorf("could not reserve vanity address: %w", err)
	}
	return nil
}

func persistVanityReservations(path string, reservations map[string]string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	payload, err := json.Marshal(reservations)
	if err != nil {
		return err
	}
	temporary := path + ".tmp"
	if err := os.WriteFile(temporary, payload, 0o600); err != nil {
		return err
	}
	return os.Rename(temporary, path)
}

func (r *relay) registerRoute(session *controlSession) (*controlSession, error) {
	r.routes.Lock()
	defer r.routes.Unlock()
	if r.byHost == nil {
		r.byHost = make(map[string]*controlSession)
	}
	if r.byDevice == nil {
		r.byDevice = make(map[string]*controlSession)
	}
	active := r.byDevice[session.deviceID]
	if active != nil && active.serverID != session.serverID {
		return nil, errors.New("your relay slot is already being used by another running server")
	}
	old := active
	if old == nil {
		old = r.byHost[session.host]
	}
	if old != nil {
		delete(r.byHost, old.host)
	}
	r.byHost[session.host] = session
	r.byDevice[session.deviceID] = session
	return old, nil
}

func (r *relay) unregisterRoute(session *controlSession) {
	r.routes.Lock()
	if r.byHost[session.host] == session {
		delete(r.byHost, session.host)
	}
	if r.byDevice[session.deviceID] == session {
		delete(r.byDevice, session.deviceID)
	}
	r.routes.Unlock()
	slog.Info("share disconnected", "host", session.host, "device", session.deviceID)
}

func (session *controlSession) send(ctx context.Context, message controlMessage) error {
	session.writeMu.Lock()
	defer session.writeMu.Unlock()
	return writeJSON(ctx, session.ws, message)
}

func writeJSON(ctx context.Context, ws *websocket.Conn, value any) error {
	payload, err := json.Marshal(value)
	if err != nil {
		return err
	}
	writeCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	return ws.Write(writeCtx, websocket.MessageText, payload)
}

func (r *relay) data(w http.ResponseWriter, request *http.Request) {
	id := request.PathValue("connection")
	token, err := base64.RawURLEncoding.DecodeString(request.URL.Query().Get("token"))
	if err != nil || len(token) != 32 {
		http.Error(w, "invalid tunnel claim", http.StatusUnauthorized)
		return
	}
	r.pending.Lock()
	pending := r.byID[id]
	if pending != nil && subtle.ConstantTimeCompare(token, pending.token) == 1 && time.Since(pending.created) <= dataConnectTimeout {
		delete(r.byID, id)
	} else {
		pending = nil
	}
	r.pending.Unlock()
	if pending == nil {
		http.Error(w, "tunnel claim expired", http.StatusNotFound)
		return
	}

	ws, err := websocket.Accept(w, request, &websocket.AcceptOptions{CompressionMode: websocket.CompressionDisabled})
	if err != nil {
		return
	}
	ws.SetReadLimit(64 * 1024)
	connection := websocket.NetConn(context.Background(), ws, websocket.MessageBinary)
	select {
	case pending.ready <- connection:
	case <-request.Context().Done():
		_ = connection.Close()
	default:
		_ = connection.Close()
	}
}

func (r *relay) acceptPlayers(listener net.Listener) {
	for {
		connection, err := listener.Accept()
		if err != nil {
			return
		}
		go r.handlePlayer(connection)
	}
}

func (r *relay) handlePlayer(player net.Conn) {
	defer player.Close()
	_ = player.SetReadDeadline(time.Now().Add(10 * time.Second))
	host, prefix, err := readMinecraftHandshake(player)
	if err != nil {
		return
	}
	_ = player.SetReadDeadline(time.Time{})
	host = normalizeMinecraftHost(host)
	r.routes.RLock()
	session := r.byHost[host]
	r.routes.RUnlock()
	if session == nil || session.active.Add(1) > maxConnections {
		if session != nil {
			session.active.Add(-1)
		}
		return
	}
	defer session.active.Add(-1)

	connectionID := randomToken(18)
	token := randomBytes(32)
	pending := &pendingConnection{token: token, created: time.Now(), ready: make(chan net.Conn, 1)}
	r.pending.Lock()
	r.byID[connectionID] = pending
	r.pending.Unlock()
	defer func() {
		r.pending.Lock()
		delete(r.byID, connectionID)
		r.pending.Unlock()
	}()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	err = session.send(ctx, controlMessage{
		Type:       "incoming",
		Connection: connectionID,
		Token:      base64.RawURLEncoding.EncodeToString(token),
	})
	cancel()
	if err != nil {
		return
	}
	var tunnel net.Conn
	select {
	case tunnel = <-pending.ready:
	case <-time.After(dataConnectTimeout):
		return
	}
	defer tunnel.Close()
	if _, err := tunnel.Write(prefix); err != nil {
		return
	}
	proxyConnections(player, tunnel)
}

func (r *relay) expirePending() {
	ticker := time.NewTicker(10 * time.Second)
	defer ticker.Stop()
	for now := range ticker.C {
		r.pending.Lock()
		for id, pending := range r.byID {
			if now.Sub(pending.created) > dataConnectTimeout {
				delete(r.byID, id)
			}
		}
		r.pending.Unlock()
	}
}

func proxyConnections(left, right net.Conn) {
	done := make(chan struct{}, 2)
	copyOne := func(dst, src net.Conn) {
		_, _ = io.Copy(dst, src)
		if tcp, ok := dst.(*net.TCPConn); ok {
			_ = tcp.CloseWrite()
		}
		done <- struct{}{}
	}
	go copyOne(left, right)
	go copyOne(right, left)
	<-done
}

func readMinecraftHandshake(reader io.Reader) (string, []byte, error) {
	var packet bytes.Buffer
	length, err := readVarInt(reader, &packet)
	if err != nil || length <= 0 || length > maxHandshakeBytes {
		return "", nil, errors.New("invalid packet length")
	}
	payload := make([]byte, length)
	if _, err := io.ReadFull(reader, payload); err != nil {
		return "", nil, err
	}
	packet.Write(payload)
	index := 0
	packetID, err := readSliceVarInt(payload, &index)
	if err != nil || packetID != 0 {
		return "", nil, errors.New("not a handshake packet")
	}
	if _, err = readSliceVarInt(payload, &index); err != nil {
		return "", nil, err
	}
	hostLength, err := readSliceVarInt(payload, &index)
	if err != nil || hostLength <= 0 || hostLength > 255 || index+hostLength > len(payload) {
		return "", nil, errors.New("invalid handshake host")
	}
	return string(payload[index : index+hostLength]), packet.Bytes(), nil
}

func readVarInt(reader io.Reader, captured *bytes.Buffer) (int, error) {
	value := 0
	for position := 0; position < 5; position++ {
		var raw [1]byte
		if _, err := io.ReadFull(reader, raw[:]); err != nil {
			return 0, err
		}
		captured.WriteByte(raw[0])
		value |= int(raw[0]&0x7f) << (7 * position)
		if raw[0]&0x80 == 0 {
			return value, nil
		}
	}
	return 0, errors.New("varint is too long")
}

func readSliceVarInt(data []byte, index *int) (int, error) {
	value := 0
	for position := 0; position < 5; position++ {
		if *index >= len(data) {
			return 0, io.ErrUnexpectedEOF
		}
		raw := data[*index]
		*index = *index + 1
		value |= int(raw&0x7f) << (7 * position)
		if raw&0x80 == 0 {
			return value, nil
		}
	}
	return 0, errors.New("varint is too long")
}

func normalizeMinecraftHost(host string) string {
	host = strings.SplitN(host, "\x00", 2)[0]
	host = strings.TrimSuffix(strings.ToLower(strings.TrimSpace(host)), ".")
	if parsedHost, _, err := net.SplitHostPort(host); err == nil {
		host = parsedHost
	}
	return host
}

func randomBytes(length int) []byte {
	value := make([]byte, length)
	if _, err := rand.Read(value); err != nil {
		panic(err)
	}
	return value
}

func randomToken(length int) string {
	return base64.RawURLEncoding.EncodeToString(randomBytes(length))
}

func encodeVarInt(value int) []byte {
	var encoded []byte
	for {
		if value&^0x7f == 0 {
			return append(encoded, byte(value))
		}
		encoded = append(encoded, byte(value&0x7f|0x80))
		value >>= 7
	}
}

func makeHandshake(host string) []byte {
	payload := append(encodeVarInt(0), encodeVarInt(767)...)
	payload = append(payload, encodeVarInt(len(host))...)
	payload = append(payload, []byte(host)...)
	payload = append(payload, 0x63, 0xdd, 0x01)
	return append(encodeVarInt(len(payload)), payload...)
}
