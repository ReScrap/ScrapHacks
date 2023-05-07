#include <stdint.h>
struct HashTable {
    uint32_t num_slots;
    struct HashTableEntry **chains;
};

struct HashTableEntry {
    void *data;
    const char *name;
    HashTableEntry *next;
};

struct World {
    void** VMT;
    HashTable *entities;
};