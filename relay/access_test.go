package main

import (
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"testing"
	"time"
)

func TestActivationCodeAuthorizesOnlyItsDevice(t *testing.T) {
	store, err := loadAccessStore(t.TempDir()+"/access.json", []byte("test-secret-that-is-long-enough-for-hmac"))
	if err != nil {
		t.Fatal(err)
	}
	codes, err := store.createCodes(1)
	if err != nil {
		t.Fatal(err)
	}
	public, _, _ := ed25519.GenerateKey(rand.Reader)
	access, err := store.activate(codes[0], public)
	if err != nil {
		t.Fatal(err)
	}
	if access.ServersAllowed != 1 || access.DeviceID != deviceIdentifier(public) {
		t.Fatalf("unexpected entitlement: %+v", access)
	}
	if _, err := store.authorize(public); err != nil {
		t.Fatalf("activated device was rejected: %v", err)
	}
	other, _, _ := ed25519.GenerateKey(rand.Reader)
	if _, err := store.authorize(other); err == nil {
		t.Fatal("unactivated device was authorized")
	}
	if _, err := store.activate(codes[0], other); err == nil {
		t.Fatal("single-use activation code was accepted twice")
	}
}

func TestActivationRequestRequiresFreshDeviceSignature(t *testing.T) {
	public, private, _ := ed25519.GenerateKey(rand.Reader)
	now := time.Now().UTC().Truncate(time.Second)
	code := "NK-AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH"
	request := activationRequest{
		ActivationCode: code,
		PublicKey:      base64.RawURLEncoding.EncodeToString(public),
		Timestamp:      now.Unix(),
	}
	request.Signature = base64.RawURLEncoding.EncodeToString(ed25519.Sign(private, activationMessage(code, request.Timestamp)))
	if _, err := verifyActivationRequest(request, now); err != nil {
		t.Fatalf("valid activation request was rejected: %v", err)
	}
	if _, err := verifyActivationRequest(request, now.Add(10*time.Minute)); err == nil {
		t.Fatal("stale activation request was accepted")
	}
}

func TestOneActiveRoutePerDeviceCanMoveAfterShutdown(t *testing.T) {
	r := &relay{}
	first := &controlSession{host: "first.example", deviceID: "device", serverID: "server-one"}
	second := &controlSession{host: "second.example", deviceID: "device", serverID: "server-two"}
	if _, err := r.registerRoute(first); err != nil {
		t.Fatal(err)
	}
	if _, err := r.registerRoute(second); err == nil {
		t.Fatal("a second server used the same relay slot")
	}
	r.unregisterRoute(first)
	if _, err := r.registerRoute(second); err != nil {
		t.Fatalf("relay slot was not released after shutdown: %v", err)
	}
}
