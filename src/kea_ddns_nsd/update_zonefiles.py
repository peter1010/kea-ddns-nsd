#!/usr/bin/python

import sys
import os
import subprocess
import fcntl

FWD_ZONEFILE = "/var/lib/nsd/home.arpa.forward"
REV_ZONEFILE = "/var/lib/nsd/home.arpa.reverse"
nsd_ctl = "/usr/sbin/nsd-control"
LOCKFILE = "/run/nsd/zone_update.lock"

ALREADY_DONE = "DONE"


def reload():
	subprocess.call([nsd_ctl, "reload"])


def read_zonefile(path):
	with open(path, "rt") as fp:
		for line in fp:
			yield line.rstrip()


def save_zonefile(path, lines):
	old = path + ".old"
	new = path + ".new"
	if os.path.exists(new):
		os.unlink(new)
	with open(new, "wt") as fp:
		fp.write("\n".join(lines))
	if os.path.exists(old):
		os.unlink(old)
	os.rename(path, old)
	os.rename(new, path)
	reload()


def update_serial(line):
	if line.endswith(";Serial"):
		num = int(line[:-7].strip()) + 1
		return "\t%i ;Serial" % num, True
	return line, False



def update_record(line, update_map, value_list):
	Changed = False
	tokens = line.split()
	if (len(tokens) != 4) or (tokens[1] != "IN"):
		return line, False
	if tokens[2] not in ("A", "PTR"):
		return line, False
	name, old_value = tokens[0], tokens[3]
	if name in update_map:
		new_value = update_map[name]
		if new_value == ALREADY_DONE:
			line = None
		elif old_value != new_value:
			line = "\t\t".join((name, tokens[1], tokens[2], new_value))
			Changed = True
		update_map[name] = ALREADY_DONE
	elif old_value in value_list:
		line = None
	return line, Changed


def update_zonefile(path, update_map):
	value_list = set(update_map.values())
	Changed = False
	for line in read_zonefile(path):
		line, done = update_serial(line)
		if not done:
			line, done = update_record(line, update_map, value_list)
			Changed = Changed or done
		if line is not None:
			yield line
	for name, value in update_map.items():
		if value != ALREADY_DONE:
			Changed = True
			if len(value.split(".")) == 4:
				_type = "A"
			else:
				_type = "PTR"
			yield "\t\t".join((name, "IN", _type, value))
	yield Changed


def update_forward_zonefile(forwards):
	lines = [line for line in update_zonefile(FWD_ZONEFILE, forwards)]
	if lines[-1]:
		save_zonefile(FWD_ZONEFILE, lines[:-1])
	

def update_reverse_zonefile(reverses):
	lines = [line for line in update_zonefile(REV_ZONEFILE, reverses)]
	if lines[-1]:
		save_zonefile(REV_ZONEFILE, lines[:-1])


def lease4_renew():
	forwards = {}
	reverses = {}

	hostname = os.getenv("LEASE4_HOSTNAME")
	ip_address = os.getenv("$LEASE4_ADDRESS")
    
	forwards[hostname] = ip_address
	reverses[ip_address] = hostname

	update_forward_zonefile(forwards)
	update_reverse_zonefile(reverses)
    

def leases4_committed():
	forwards = {}
	reverses = {}
	number = int(os.getenv("LEASES4_SIZE"))
	for x in range(number):
		hostname = os.getenv("LEASES4_AT%i_HOSTNAME" % x)
		ip_address = os.getenv("LEASES4_AT%i_ADDRESS" % x)
		forwards[hostname] = ip_address
		reverses[ip_address] = hostname
	update_forward_zonefile(forwards)
	update_reverse_zonfile(reverses)


def AquireLock():
	lockfd = open(LOCKFILE, "a+")
	fcntl.flock(lockfd.fileno(), fcntl.LOCK_EX)


def main():
	AquireLock()
	if len(sys.argv) > 1:
		action = sys.argv[1]

		if action == "lease4_renew":
			lease_renew()
		elif action == "lease4_recover":
			lease_renew()
		elif action == "leases4_committed":
			leases_committed()
#	else:
#		update_map = { "frodo" : "192.168.11.26" }
#		update_forward_zonefile(update_map)

#		update_map = { "11" : "camera" }
#		update_reverse_zonefile(update_map)

if __name__ == "__main__":
	main()
