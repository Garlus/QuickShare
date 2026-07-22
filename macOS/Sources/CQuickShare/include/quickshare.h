#ifndef quickshare_h
#define quickshare_h

#include <stdint.h>

// Opaque context handle
typedef struct QsContext QsContext;

// Callback types
typedef void (*qs_device_found_cb_t)(const char* device_id,
                                      const char* device_name,
                                      int connection_type,
                                      void* user_data);

typedef void (*qs_transfer_cb_t)(const char* transfer_id,
                                  const char* device_id,
                                  int status,
                                  int64_t bytes_sent,
                                  int64_t bytes_total,
                                  void* user_data);

typedef void (*qs_log_cb_t)(int level,
                             const char* message,
                             void* user_data);

typedef void (*qs_incoming_transfer_cb_t)(const char* request_id,
                                           const char* device_name,
                                           const char* file_name,
                                           int64_t file_size,
                                           int file_number,
                                           void* user_data);

// Lifecycle
QsContext* qs_init(const char* device_name, qs_log_cb_t log_cb, void* log_user_data);
void qs_shutdown(QsContext* ctx);

// Callbacks
void qs_set_device_found_callback(QsContext* ctx,
                                   qs_device_found_cb_t cb,
                                   void* user_data);

void qs_set_transfer_callback(QsContext* ctx,
                               qs_transfer_cb_t cb,
                               void* user_data);

void qs_set_incoming_transfer_callback(QsContext* ctx,
                                        qs_incoming_transfer_cb_t cb,
                                        void* user_data);

// Incoming transfer accept/deny
int qs_accept_transfer(const char* request_id);
int qs_deny_transfer(const char* request_id);

// Discovery
int qs_start_advertising(QsContext* ctx, int device_type);
int qs_stop_advertising(QsContext* ctx);
int qs_start_discovery(QsContext* ctx);
int qs_stop_discovery(QsContext* ctx);

// BLE advertising — returns base64url-encoded 4-byte endpoint ID (caller must qs_free_string)
char* qs_get_endpoint_id(QsContext* ctx);

// TCP listener for incoming transfers
int qs_start_listener(QsContext* ctx, const char* save_dir);
int qs_stop_listener(QsContext* ctx);

// File sending (blocking — call from background thread)
int qs_send_file(QsContext* ctx, const char* device_ip, int port, const char* endpoint_id, const char* file_path);

// Status
int qs_is_advertising(QsContext* ctx);
int qs_is_discovering(QsContext* ctx);

// Utils
const char* qs_version(void);
void qs_free_string(char* s);

#endif /* quickshare_h */
