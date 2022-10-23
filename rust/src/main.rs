use std::os::unix::net::UnixDatagram;
use std::process::Command;
use std::io::Write;
use std::io::Read;
use std::fs::{OpenOptions, File, rename, remove_file};
use std::env;
use std::collections::{HashMap, HashSet};
use phf::phf_map;

//use fcntl::{is_file_locked, lock_file, unlock_file};

static FWD_ZONEFILE : &'static str = "/var/lib/nsd/home.arpa.forward";
static REV_ZONEFILE : &'static str = "/var/lib/nsd/home.arpa.reverse";
static NSD_CTL : &'static str = "/usr/sbin/nsd-control";
static LOCKFILE : &'static str = "/run/kea/zone_update.lock";
static SERIAL : &'static str = ";Serial";
static ALREADY_DONE : &'static str = "DONE";


//# For those devices that refuse to give away there names
static STATIC_MAPPINGS : phf::Map<&'static str, &'static str> = phf_map! {
    "24_46_c8_8b_bb_f1" => "motoG7"
};

type UpdateDict = HashMap<String, String>;
type DoneSet = HashSet<String>;

///	Send message to Syslog
fn syslog(msg : &str) 
{
	let prio = 5;	// Notice
	let facility = 1; // # User
    let data = format!("<{}> kea-ddns-nsd {}", (prio + 8 * facility),  msg);

    match UnixDatagram::unbound() {
        Ok(sock) => {
            match sock.connect("/dev/log") {
                Ok(_) => {
                    if sock.send(data.as_bytes()).is_err() {
                        println!("Syslog send error"); 
                        return  // return early..
                    }
                },
                Err(error) => {
                    println!("Syslog connect error {}", error);
                    return;
                }
            };
        },
        Err(error) => {
            println!("Syslog socket error {}", error);
            return;
        }
    };
}


///	Signal to NSD to reload the zonefiles
fn reload() 
{
    Command::new(NSD_CTL).arg("reload");
//	subprocess.call([nsd_ctl, "reload"])
}


/// Implement an iterator, 
///	with open(path, "rt") as fp :
///		for line in fp:
///			yield line.rstrip()
///
    
struct ReadZoneFile 
{
    fp : File
}


impl Iterator for ReadZoneFile {
    type Item = String;


    fn next(&mut self) -> Option<Self::Item>
    {
        let mut data = String::new();
        match self.fp.read_to_string(&mut data) {
            Ok(_) => Some(String::from(data.trim_end())),
            Err(_) => None
        }
    }
}


fn read_zonefile(path : &str) -> ReadZoneFile
{
    let fp = match File::open(path) {
        Ok(fp) => fp,
        Err(_) => {
            syslog(&format!("Failed to open {}", path));
            panic!("Failed to open {}", path);
        }
    };

    ReadZoneFile {fp : fp}
}


///	Save zonefile for use by NSD.
///	The current saved zonefile is rename with a .old extension"""
fn save_zonefile(path : &str, lines : &Vec<String>) 
{
	let old = format!("{}{}", path, ".old");
	let new = format!("{}{}", path, ".new");

    _ = remove_file(&new);

    let mut fp = match File::create(&new) {
        Ok(file) => file,
        Err(_) => {
            syslog(&format!("Failed to create {}", new));
            return;    // return early
        }
    };

    for line in lines {
        if writeln!(fp, "{}", line).is_err() {
            syslog(&format!("Failed to write to {}", new));
            return;
        }
    }

    _ = remove_file(&old);
    _ = rename(path, &old);
    if rename(&new, path).is_err() {
        syslog(&format!("Failed to rename {} to {}", new, path));
        return;
    }
	reload();
}


fn update_serial(line :&str) -> (Option<String>, bool) 
{
	if line.ends_with(SERIAL) {

		let num : u32 = line[..(line.len() - SERIAL.len())].trim().parse::<u32>().unwrap() + 1;
		return (Some(format!("\t{} {}", SERIAL, num)), true);
    }
	return (Some(String::from(line)), false);
}


fn update_record(line : &str, update_map : &UpdateDict, value_list : &HashSet<&str>, done_set : &mut DoneSet) -> (Option<String>, bool)
{
	let mut tokens = line.split_whitespace();
    // Default result
    let result = (Some(String::from(line)), false);

    // First token is name
    let name = tokens.next();

    // second token is "IN"
    if tokens.next() != Some("IN") {
        return result;
    }
    let name = name.unwrap();

    // third token is either "A" or "PTR"
    let rec_type = tokens.next();
    if rec_type != Some("A") && rec_type != Some("PTR") {
        return result;
    }
    let rec_type = rec_type.unwrap();

    // fourth token is the value
    let old_value = match tokens.next() {
        Some(old_value) => old_value,
        None => return result
    };

    // Check no more tokens..
    if tokens.next() != None {
        return result;
    }

    // Have we already seen and done this entry?
    if done_set.contains(name) {
        return (None, false);
    }
    done_set.insert(String::from(name));

	match update_map.get(name) {
        Some(new_value) => {
            if old_value != new_value {
			    return (Some(format!("{}\t\t{}\t{}\t{}", name, "IN", rec_type, new_value)), true);
            }
        }
        None => {
	        if value_list.contains(old_value) {
                return (None, false);
            }
        }
    }
	return result;
}


fn update_zonefile(path : &str, update_map : &UpdateDict, _type : &str) -> (Vec<String>, bool)
{
    let mut lines = Vec::new();
	let mut value_set : HashSet<&str> = HashSet::new();
    let mut done_set : DoneSet = HashSet::new();

    for name in update_map.keys() {
        value_set.insert(&update_map[name].as_str());
    }

	let mut changed = false;
	for line in read_zonefile(path) {
		let (mut new_line, mut done) = update_serial(&line);
		if !done {
			(new_line, done) = update_record(&line, &update_map, &value_set, &mut done_set);
			changed = changed || done;
        }
		if new_line != None {
			lines.push(new_line.unwrap());
        }
    }
	for (name, value) in update_map {
		if value != ALREADY_DONE {
			changed = true;
			lines.push( format!("{}\t\t{}\t{}\t{}" , name, "IN", _type, value));
        }
    }
	return (lines, changed);
}


fn update_forward_zonefile(forwards : &UpdateDict) 
{
	let (lines, changed) = update_zonefile(FWD_ZONEFILE, forwards, "A");
	if changed {
		save_zonefile(FWD_ZONEFILE, &lines);
    }
}


fn update_reverse_zonefile(reverses : &UpdateDict) 
{
	let (lines, changed) = update_zonefile(REV_ZONEFILE, reverses, "PTR");
	if changed {
		save_zonefile(REV_ZONEFILE, &lines);
    }
}

///	Given a hostname and it's hardware address, return a
///	cleaned up version of the hostname
fn clean_hostname(hostname : &str, hw_addr : &str) -> String 
{
	if hostname.len() == 0 {
		let hw_addr = hw_addr.replace(":","_");
		return match STATIC_MAPPINGS.get(&hw_addr) {
            Some(value) => String::from(*value),
            None => hw_addr
        };
	}
	return match hostname.find(".") {
	    Some(idx) => hostname[..idx].to_string(),
        None => String::from(hostname)
    };
}


fn lease4_renew() {
	//hostname = os.getenv("LEASE4_HOSTNAME")
    let hostname = match env::var("LEASE4_HOSTNAME") {
        Ok(val) => val,
        Err(_) => String::new()
    };

	//ip_address = os.getenvar("LEASE4_ADDRESS")
	let ip_address = match env::var("LEASE4_ADDRESS") {
        Ok(val) => val,
        Err(_) => {
		    syslog("No IP address specified in lease renewal");
            return
        }
    };

	//if ip_address is None:
	//	syslog("No IP address specified in lease renewal")
	//	return

	let parts = ip_address.rsplit_once(".").unwrap();

	let hw_addr = env::var("LEASE4_HWADDR").unwrap();

	let hostname = clean_hostname(&hostname, &hw_addr);

    let mut forwards = HashMap::new();
    let mut reverses = HashMap::new();
	
	syslog(&format!("{} -> {}", &hostname, &ip_address));

	forwards.insert(String::from(&hostname), String::from(&ip_address));
	reverses.insert(String::from(parts.1), hostname + ".home.arpa.");

	update_forward_zonefile(&mut forwards);
	update_reverse_zonefile(&mut reverses);
}    


fn leases4_committed() {
//	forwards = {}
    let mut forwards = HashMap::new();
//	reverses = {}
    let mut reverses = HashMap::new();
//	number = int(os.getenv("LEASES4_SIZE"))
	let number = env::var("LEASES4_SIZE").unwrap().trim().parse().unwrap();
	for x in 0..number {
		// hostname = os.getenv("LEASES4_AT%i_HOSTNAME" % x)
        let hostname = match env::var(format!("LEASES4_AT{}_HOSTNAME", x)) {
            Ok(val) => val,
            Err(_) => String::new()
        };

		// ip_address = os.getenv("LEASES4_AT%i_ADDRESS" % x)
    	let ip_address = match env::var(format!("LEASES4_AT{}_ADDRESS", x)) {
            Ok(val) => val,
            Err(_) => {
		        syslog("No IP address specified in lease renewal");
                continue
            }
        };


		//if ip_address is None:
		//	syslog("No IP address specified in lease renewal")
		//	continue

		let parts = ip_address.rsplit_once(".").unwrap();
	
        let hw_addr = env::var(format!("LEASES4_AT{}_HWADDR", x)).unwrap();

		let hostname = clean_hostname(&hostname, &hw_addr);

		syslog(&format!("{} -> {}", hostname, ip_address));

		forwards.insert(String::from(&hostname), String::from(&ip_address));
		reverses.insert(String::from(parts.1), hostname);
    }
	update_forward_zonefile(&mut forwards);
	update_reverse_zonefile(&mut reverses);
}

fn aquire_lock() {
    let lockfd = match OpenOptions::new().append(true).create(true).open(LOCKFILE) {
        Ok(file) => file,
        Err(_) => panic!("Failed to lock file")
    };

//	try:
//		lockfd = File::open(LOCKFILE, "a+")
//	except PermissionError:
//		syslog("Failed to acquire lock")
//		sys.exit(1)

//    lock_file(&lockfd, None, Some(FcntlLockType.Write));

//	fcntl.flock(lockfd.fileno(), fcntl.LOCK_EX)
//	# fcntl.LOCK_NB
//	try:
//		os.chmod(LOCKFILE, 0o777)
//	except PermissionError:
//		pass
}

fn main() {
	aquire_lock();
    let args : Vec<String> = env::args().collect();
	if args.len() > 1 {
		let action = &args[1];

		match action.as_str() {
            "lease4_renew" => lease4_renew(),
		    "lease4_recover" => lease4_renew(),
	        "leases4_committed" => leases4_committed(),
		    _ => syslog(&format!("Action {} ignored", &action))
        }
	} else {
		syslog("No action specified");

		let mut update_map : UpdateDict = HashMap::new();
        update_map.insert(String::from("frodo"), String::from("192.168.11.26"));
		update_forward_zonefile(&update_map)

//		update_map = { "11" : "camera" }
//		update_reverse_zonefile(update_map)
    }
}

