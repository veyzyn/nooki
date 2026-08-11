package main

import (
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
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

const (
	activationProtocol = "nooki-relay-activation-v1"
	maximumClockSkew   = 2 * time.Minute
	maximumAdminCodes  = 100
)

type accessStore struct {
	mu     sync.Mutex
	path   string
	secret []byte
	data   accessDatabase
}

type accessDatabase struct {
	Version      int                    `json:"version"`
	Codes        map[string]accessCode  `json:"codes"`
	Entitlements map[string]entitlement `json:"entitlements"`
}

type accessCode struct {
	CreatedAt    time.Time  `json:"createdAt"`
	RedeemedAt   *time.Time `json:"redeemedAt,omitempty"`
	DeviceID     string     `json:"deviceId,omitempty"`
	ActivationID string     `json:"activationId,omitempty"`
}

type entitlement struct {
	ActivationID   string     `json:"activationId"`
	DeviceID       string     `json:"deviceId"`
	PublicKey      string     `json:"publicKey"`
	ActivatedAt    time.Time  `json:"activatedAt"`
	RevokedAt      *time.Time `json:"revokedAt,omitempty"`
	ServersAllowed int        `json:"serversAllowed"`
}

type activationRequest struct {
	ActivationCode string `json:"activationCode"`
	PublicKey      string `json:"publicKey"`
	Signature      string `json:"signature"`
	Timestamp      int64  `json:"timestamp"`
}

type activationResponse struct {
	Activated      bool   `json:"activated"`
	ActivationID   string `json:"activationId"`
	DeviceID       string `json:"deviceId"`
	ServersAllowed int    `json:"serversAllowed"`
}

type activationError struct {
	Message string `json:"message"`
}

type createCodesRequest struct {
	Count int `json:"count"`
}

type createCodesResponse struct {
	Codes []string `json:"codes"`
}

func loadAccessStore(path string, secret []byte) (*accessStore, error) {
	database := accessDatabase{
		Version:      1,
		Codes:        make(map[string]accessCode),
		Entitlements: make(map[string]entitlement),
	}
	payload, err := os.ReadFile(path)
	if err == nil {
		if err := json.Unmarshal(payload, &database); err != nil {
			return nil, fmt.Errorf("decode relay access database: %w", err)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}
	if database.Version != 1 {
		return nil, fmt.Errorf("unsupported relay access database version %d", database.Version)
	}
	if database.Codes == nil {
		database.Codes = make(map[string]accessCode)
	}
	if database.Entitlements == nil {
		database.Entitlements = make(map[string]entitlement)
	}
	return &accessStore{path: path, secret: append([]byte(nil), secret...), data: database}, nil
}

func (store *accessStore) authorize(publicKey []byte) (entitlement, error) {
	deviceID := deviceIdentifier(publicKey)
	store.mu.Lock()
	defer store.mu.Unlock()
	access, ok := store.data.Entitlements[deviceID]
	if !ok || access.RevokedAt != nil || access.PublicKey != base64.RawURLEncoding.EncodeToString(publicKey) {
		return entitlement{}, errors.New("relay access is not activated for this device")
	}
	return access, nil
}

func (store *accessStore) activate(code string, publicKey []byte) (entitlement, error) {
	normalized, err := normalizeActivationCode(code)
	if err != nil {
		return entitlement{}, err
	}
	deviceID := deviceIdentifier(publicKey)
	encodedKey := base64.RawURLEncoding.EncodeToString(publicKey)
	hash := store.codeHash(normalized)

	store.mu.Lock()
	defer store.mu.Unlock()
	if existing, ok := store.data.Entitlements[deviceID]; ok && existing.RevokedAt == nil {
		if existing.PublicKey != encodedKey {
			return entitlement{}, errors.New("device identity does not match its activation")
		}
		return existing, nil
	}
	record, ok := store.data.Codes[hash]
	if !ok {
		return entitlement{}, errors.New("that activation key is invalid")
	}
	if record.RedeemedAt != nil {
		return entitlement{}, errors.New("that activation key has already been used")
	}
	now := time.Now().UTC()
	access := entitlement{
		ActivationID:   "NA-" + strings.ToUpper(randomBase32(8)),
		DeviceID:       deviceID,
		PublicKey:      encodedKey,
		ActivatedAt:    now,
		ServersAllowed: 1,
	}
	record.RedeemedAt = &now
	record.DeviceID = deviceID
	record.ActivationID = access.ActivationID
	store.data.Codes[hash] = record
	store.data.Entitlements[deviceID] = access
	if err := store.persistLocked(); err != nil {
		delete(store.data.Entitlements, deviceID)
		record.RedeemedAt = nil
		record.DeviceID = ""
		record.ActivationID = ""
		store.data.Codes[hash] = record
		return entitlement{}, err
	}
	return access, nil
}

func (store *accessStore) createCodes(count int) ([]string, error) {
	if count < 1 || count > maximumAdminCodes {
		return nil, fmt.Errorf("count must be between 1 and %d", maximumAdminCodes)
	}
	store.mu.Lock()
	defer store.mu.Unlock()
	created := make([]string, 0, count)
	for len(created) < count {
		raw := strings.ToUpper(randomBase32(20))
		code := "NK-" + groupCode(raw)
		hash := store.codeHash("NK" + raw)
		if _, exists := store.data.Codes[hash]; exists {
			continue
		}
		store.data.Codes[hash] = accessCode{CreatedAt: time.Now().UTC()}
		created = append(created, code)
	}
	if err := store.persistLocked(); err != nil {
		for _, code := range created {
			normalized, _ := normalizeActivationCode(code)
			delete(store.data.Codes, store.codeHash(normalized))
		}
		return nil, err
	}
	return created, nil
}

func (store *accessStore) codeHash(code string) string {
	mac := hmac.New(sha256.New, store.secret)
	_, _ = mac.Write([]byte(code))
	return hex.EncodeToString(mac.Sum(nil))
}

func (store *accessStore) persistLocked() error {
	if err := os.MkdirAll(filepath.Dir(store.path), 0o700); err != nil {
		return err
	}
	payload, err := json.MarshalIndent(store.data, "", "  ")
	if err != nil {
		return err
	}
	temporary := store.path + ".tmp"
	if err := os.WriteFile(temporary, payload, 0o600); err != nil {
		return err
	}
	return os.Rename(temporary, store.path)
}

func (r *relay) activate(w http.ResponseWriter, request *http.Request) {
	request.Body = http.MaxBytesReader(w, request.Body, 16*1024)
	var input activationRequest
	if err := json.NewDecoder(request.Body).Decode(&input); err != nil {
		writeActivationError(w, http.StatusBadRequest, "The activation request was invalid.")
		return
	}
	publicKey, err := verifyActivationRequest(input, time.Now())
	if err != nil {
		writeActivationError(w, http.StatusUnauthorized, err.Error())
		return
	}
	access, err := r.access.activate(input.ActivationCode, publicKey)
	if err != nil {
		writeActivationError(w, http.StatusConflict, err.Error())
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(activationResponse{
		Activated: true, ActivationID: access.ActivationID, DeviceID: access.DeviceID, ServersAllowed: access.ServersAllowed,
	})
}

func (r *relay) createActivationCodes(w http.ResponseWriter, request *http.Request) {
	provided := strings.TrimPrefix(request.Header.Get("Authorization"), "Bearer ")
	if len(provided) != len(r.config.adminToken) || subtle.ConstantTimeCompare([]byte(provided), []byte(r.config.adminToken)) != 1 {
		writeActivationError(w, http.StatusUnauthorized, "Unauthorized.")
		return
	}
	request.Body = http.MaxBytesReader(w, request.Body, 4*1024)
	input := createCodesRequest{Count: 1}
	if request.ContentLength != 0 {
		if err := json.NewDecoder(request.Body).Decode(&input); err != nil {
			writeActivationError(w, http.StatusBadRequest, "The request was invalid.")
			return
		}
	}
	codes, err := r.access.createCodes(input.Count)
	if err != nil {
		writeActivationError(w, http.StatusBadRequest, err.Error())
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(createCodesResponse{Codes: codes})
}

func verifyActivationRequest(input activationRequest, now time.Time) ([]byte, error) {
	publicKey, err := base64.RawURLEncoding.DecodeString(input.PublicKey)
	if err != nil || len(publicKey) != ed25519.PublicKeySize {
		return nil, errors.New("The device identity was invalid.")
	}
	signature, err := base64.RawURLEncoding.DecodeString(input.Signature)
	if err != nil || len(signature) != ed25519.SignatureSize {
		return nil, errors.New("The device signature was invalid.")
	}
	when := time.Unix(input.Timestamp, 0)
	if when.Before(now.Add(-maximumClockSkew)) || when.After(now.Add(maximumClockSkew)) {
		return nil, errors.New("The computer clock is too far out of sync.")
	}
	message := activationMessage(input.ActivationCode, input.Timestamp)
	if !ed25519.Verify(ed25519.PublicKey(publicKey), message, signature) {
		return nil, errors.New("The device signature was rejected.")
	}
	return publicKey, nil
}

func activationMessage(code string, timestamp int64) []byte {
	return []byte(fmt.Sprintf("%s\n%s\n%d", activationProtocol, strings.TrimSpace(code), timestamp))
}

func normalizeActivationCode(value string) (string, error) {
	value = strings.ToUpper(strings.TrimSpace(value))
	value = strings.ReplaceAll(value, "-", "")
	if len(value) != 34 || !strings.HasPrefix(value, "NK") {
		return "", errors.New("that activation key is invalid")
	}
	for _, character := range value {
		if (character < 'A' || character > 'Z') && (character < '2' || character > '7') {
			return "", errors.New("that activation key is invalid")
		}
	}
	return value, nil
}

func groupCode(value string) string {
	groups := make([]string, 0, (len(value)+3)/4)
	for len(value) > 0 {
		length := min(4, len(value))
		groups = append(groups, value[:length])
		value = value[length:]
	}
	return strings.Join(groups, "-")
}

func randomBase32(size int) string {
	bytes := make([]byte, size)
	if _, err := rand.Read(bytes); err != nil {
		panic(err)
	}
	return base32.StdEncoding.WithPadding(base32.NoPadding).EncodeToString(bytes)
}

func deviceIdentifier(publicKey []byte) string {
	hash := sha256.Sum256(publicKey)
	return hex.EncodeToString(hash[:8])
}

func writeActivationError(w http.ResponseWriter, status int, message string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(activationError{Message: message})
}
