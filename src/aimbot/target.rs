use INTERFACES;
use OFFSETS;
use offsets::ptr_offset;
use libc;
use sdk::{self, Ray_t, trace_t, Vector};
use std::mem;

const TRIGGER_MASK: libc::c_uint = 0x4200400B; 

pub struct Target {
    pub pos: Vector
}

#[allow(dead_code)]
pub struct Targets {
    current_entnum: libc::c_int,
    highest_entnum: libc::c_int,
}
impl Targets {
    pub unsafe fn new() -> Targets {
        Targets {
            current_entnum: 0,
            highest_entnum: 
                sdk::CEntList_GetHighestEntityIndex(INTERFACES.entlist) 
        
        }
    }

}

impl Iterator for Targets {
    type Item = Target;
    fn next(&mut self) -> Option<Target> {
        while self.current_entnum < self.highest_entnum {
            let targ = unsafe {
                let kek = self.current_entnum;
                self.get_target(kek)
            };

            self.current_entnum += 1;

            if targ.is_some() {
                return targ;
            }

        }
        None
    }

}
impl Targets {
    unsafe fn get_target(&mut self, entnum: libc::c_int) -> Option<Target> {
        use std::ffi::{CStr, CString};
        let ent = sdk::CEntList_GetClientEntity(INTERFACES.entlist, entnum);
        if ent.is_null() {
            return None;
        }
        let dormant = sdk::CBaseEntity_IsDormant(ent); 
        if dormant { return None;
        }
        let class = sdk::CBaseEntity_GetClientClass(ent);
        let classname = CStr::from_ptr((*class).name); 
        let (targettable, is_player) = match classname.to_bytes() {
            b"CTFPlayer" => (true, true),
            b"CObjectSentrygun" | b"CTFTankBoss" => (true, false),
            _ => (false, false) 
        };
        if !targettable { 
            return None;
        }

        let me_idx = sdk::EngineClient_GetLocalPlayer(INTERFACES.engine);
        let me = sdk::CEntList_GetClientEntity(INTERFACES.entlist, me_idx);
        let myteam = *ptr_offset::<_, libc::c_int>(me, OFFSETS.m_iTeamNum);
        let friendly = *ptr_offset::<_, libc::c_int>(ent, OFFSETS.m_iTeamNum) == myteam;
        let alive = *ptr_offset::<_, i8>(ent, OFFSETS.m_lifeState) == 0;
        let condok = if is_player {
            let cond = *ptr_offset::<_, libc::c_int>(ent, OFFSETS.m_nPlayerCond);
            let condex = *ptr_offset::<_, libc::c_int>(ent, OFFSETS.m_nPlayerCondEx);
            (cond & (1<<14 | 1<<5 | 1<<13) == 0) && (condex & (1<<19) == 0) 
        } else {
            true
        };

        if !friendly && alive && condok {
            let targtime = ((*INTERFACES.globals).interval_per_tick * ((*INTERFACES.globals).tickcount as f32));
            let oldsimtime = *ptr_offset::<_, f32>(ent, OFFSETS.m_flSimulationTime);
            let oldanimtime = *ptr_offset::<_, f32>(ent, OFFSETS.m_flAnimTime);

            if is_player {
                *ptr_offset::<_, f32>(ent, OFFSETS.m_flSimulationTime) = targtime;
                *ptr_offset::<_, f32>(ent, OFFSETS.m_flAnimTime) = targtime;
                let bone_matrices = super::bone::get_all_bone_matrices(ent, targtime);
                *ptr_offset::<_, f32>(ent, OFFSETS.m_flSimulationTime) = oldsimtime;
                *ptr_offset::<_, f32>(ent, OFFSETS.m_flAnimTime) = oldanimtime;
                let modelptr = sdk::CBaseEntity_GetModel(ent);
                if modelptr.is_null() { return None; }
                let studiomdl = sdk::IVModelInfo_GetStudiomodel(INTERFACES.modelinfo, modelptr); 
                if studiomdl.is_null() { return None; }
                let hitboxsets = ::std::slice::from_raw_parts(
                    (studiomdl as usize + (*studiomdl).hitboxsetindex as usize) as *const sdk::mstudiohitboxset_t,
                    (*studiomdl).numhitboxsets as usize);
                let hitboxset = &hitboxsets[0];
                let hitboxes = ::std::slice::from_raw_parts(
                    (hitboxset as *const _ as usize + hitboxset.hitboxindex as usize) as *const sdk::mstudiobbox_t,
                    (*hitboxset).numhitboxes as usize);

                for hitbox in hitboxes.iter().take(1) {
                    let bone = hitbox.bone as usize;
                    let max = bone_matrices[bone].transform_point(&hitbox.bbmax);
                    let min = bone_matrices[bone].transform_point(&hitbox.bbmin);
                    let center = (max + min).scale(0.5);
                    if self.is_visible(me, ent, center) {
                        return Some(Target { pos: center });
                    }
                }
                None
            } else {
                let mut targpos = Vector { x: 0., y: 0., z: 0. };
                sdk::CBaseEntity_GetWorldSpaceCenter(ent, &mut targpos);
                if self.is_visible(me, ent, targpos) {
                    Some(Target { pos: targpos })
                } else {
                    None
                }
            }
        } else {
            None
        }  
    }
    unsafe fn is_visible(&self, me: *mut sdk::CBaseEntity, ent: *mut sdk::CBaseEntity, targpos: sdk::Vector) -> bool {
            let meorigin = sdk::CBaseEntity_GetAbsOrigin(me).clone();
            let eyes = meorigin + *ptr_offset::<_, Vector>(me, OFFSETS.m_vecViewOffset);

            let ray = Ray_t::new(eyes, targpos);
            let mut tr = mem::zeroed::<trace_t>();
            sdk::CTraceFilterSkipEntity_SetHandle(sdk::GLOBAL_TRACEFILTER_PTR, *sdk::CBaseEntity_GetRefEHandle(me));

            sdk::CEngineTrace_TraceRay(INTERFACES.trace,
                                       &ray,
                                       TRIGGER_MASK,
                                       sdk::GLOBAL_TRACEFILTER_PTR,
                                       &mut tr);
            tr.ent == ent || tr.fraction > 0.985
    }
}
